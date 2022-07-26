#![feature(async_closure)]
#![feature(let_chains)]

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
    LogSpecification,
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
};
use tracing_subscriber::FmtSubscriber;

#[macro_use]
extern crate lazy_static;

fn parse_args() -> Result<Option<PathBuf>, lexopt::Error> {
    let mut config: Option<PathBuf> = None;
    let mut parser = lexopt::Parser::from_env();
    while let Some(arg) = parser.next()? {
        match arg {
            Short('c') | Long("config") => {
                config = Some(PathBuf::from(parser.value()?));
            }
            Long("help") => {
                println!("Usage: rsvc [-c|--config=CONFIG]");
                std::process::exit(0);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    Ok(config)
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
    let config_file = parse_args()?;
    let config = Config::new(config_file)?;

    // Setup logging
    let (file_writer, _fw_handle) = FileLogWriter::builder(
        FileSpec::default()
            .directory(config.logdir.as_ref().unwrap())
            .basename("rinit"),
    )
    .rotate(
        Criterion::Size(1024 * 512),
        Naming::Numbers,
        Cleanup::KeepLogAndCompressedFiles(1, 4),
    )
    .append()
    .write_mode(WriteMode::Async)
    .try_build_with_handle()
    .unwrap();

    let env_filter = LogSpecification::env()?.to_string();
    let subscriber_builder = FmtSubscriber::builder()
        .with_writer(move || file_writer.clone())
        .with_env_filter(
            if env_filter.is_empty() {
                "info"
            } else {
                &env_filter
            },
        );

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
