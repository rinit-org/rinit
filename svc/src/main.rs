#![feature(async_closure)]

pub mod live_service;
pub mod live_service_graph;
pub mod request_handler;

use std::{
    path::{
        Path,
        PathBuf,
    },
    rc::Rc,
};

use anyhow::Result;
use flexi_logger::{
    writers::FileLogWriter,
    Cleanup,
    Criterion,
    FileSpec,
    Naming,
    WriteMode,
};
use lexopt::prelude::{
    Long,
    Short,
};
use live_service_graph::LiveServiceGraph;
use request_handler::RequestHandler;
use rinit_ipc::Request;
use rinit_service::{
    config::Config,
    types::RunLevel,
};
use tokio::{
    fs,
    net::UnixListener,
    select,
    signal::unix::{
        signal,
        Signal,
        SignalKind,
    },
    sync::Mutex,
    task::{
        self,
        JoinError,
    },
};
use tracing::{
    error,
    info,
    metadata::LevelFilter,
};
use tracing_subscriber::FmtSubscriber;

#[macro_use]
extern crate lazy_static;

struct Args {
    config: Option<PathBuf>,
    verbosity: u8,
}

fn parse_args() -> Result<Args, lexopt::Error> {
    let mut config: Option<PathBuf> = None;
    let mut parser = lexopt::Parser::from_env();
    // 0 => Error
    // 1 => Warn
    // 2 => Info
    // 3 => Debug
    // 4 => Trace
    let mut verbosity = 2;
    while let Some(arg) = parser.next()? {
        match arg {
            Short('c') | Long("config") => {
                config = Some(PathBuf::from(parser.value()?));
            }
            Long("help") => {
                println!("Usage: rsvc [-c|--config=CONFIG]");
                std::process::exit(0);
            }
            Short('q') | Long("quiet") => {
                // quiet set it to Warn
                verbosity = 1;
            }
            Short('v') | Long("verbose") => {
                // verbose set it to Debug
                verbosity = 3;
            }
            _ => return Err(arg.unexpected()),
        }
    }

    Ok(Args { config, verbosity })
}

lazy_static! {
    static ref SIGINT: Mutex<Signal> = Mutex::new(signal(SignalKind::interrupt()).unwrap());
    static ref SIGTERM: Mutex<Signal> = Mutex::new(signal(SignalKind::terminate()).unwrap());
}

pub async fn signal_wait() {
    let mut sigint = SIGINT.lock().await;
    let mut sigterm = SIGTERM.lock().await;
    select! {
        _ = sigint.recv() => {},
        _ = sigterm.recv() => {},
    };
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = parse_args()?;
    let config = Config::new(args.config)?;

    // Setup logging
    let (file_writer, _fw_handle) = FileLogWriter::builder(
        FileSpec::default()
            .directory(config.logdir.as_ref().unwrap())
            .basename("rinit"),
    )
    .rotate(
        Criterion::Size(1024 * 512),
        Naming::Numbers,
        Cleanup::KeepCompressedFiles(5),
    )
    .append()
    .write_mode(WriteMode::Async)
    .try_build_with_handle()
    .unwrap();

    let subscriber_builder = FmtSubscriber::builder()
        .with_writer(move || file_writer.clone())
        .with_max_level(match args.verbosity {
            0 => LevelFilter::ERROR,
            1 => LevelFilter::WARN,
            2 => LevelFilter::INFO,
            3.. => LevelFilter::DEBUG,
        });

    // Get ready to trace
    tracing::subscriber::set_global_default(subscriber_builder.finish())
        .expect("setting default subscriber failed");

    let local = task::LocalSet::new();
    let live_graph = LiveServiceGraph::new(config)?;

    // Setup socket listener
    fs::create_dir_all(Path::new(rinit_ipc::get_host_address()).parent().unwrap())
        .await
        .unwrap();

    local
        .run_until(async move {
            info!("Starting rinit!");
            let listener = Rc::new(UnixListener::bind(rinit_ipc::get_host_address()).unwrap());
            let handler = Rc::new(RequestHandler::new(live_graph));

            let handler_clone = handler.clone();
            let mut handles = vec![task::spawn_local(async move {
                if let Err(err) = handler_clone
                    .handle(Request::StartAllServices(RunLevel::Boot))
                    .await
                {
                    error!("{err}");
                }

                if let Err(err) = handler_clone
                    .handle(Request::StartAllServices(RunLevel::Default))
                    .await
                {
                    error!("{err}");
                }
            })];

            loop {
                select! {
                    // put signal_wait first because we want to stop as soon as
                    // we receive a termination signal
                    // this is cancel safe
                    _ = signal_wait() => {
                        for handle in handles {
                            let res: Result<(), JoinError> = handle.await;
                            res.unwrap();
                        }
                        break;
                    }
                    // this is cancel safe
                    conn = listener.accept() => {
                        let stream = match conn {
                            Ok((stream, _addr)) => stream,
                            Err(err) => {
                                error!("error while accepting a new connection: {err}");
                                return;
                            },
                        };
                        let handler = handler.clone();
                        handles.push(task::spawn_local(async move {
                            if let Err(err) = handler.handle_stream(stream).await {
                                error!("{err}");
                            }
                        }));
                    }
                }
            }
        })
        .await;

    fs::remove_file(rinit_ipc::get_host_address())
        .await
        .unwrap();

    Ok(())
}
