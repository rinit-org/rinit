#![feature(async_closure)]

pub mod live_service;
pub mod live_service_graph;
pub mod request_handler;

use std::{
    cell::RefCell,
    path::{
        Path,
        PathBuf,
    },
    rc::Rc,
};

use anyhow::{
    Context,
    Result,
};
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
use nix::{
    sys::signal::Signal,
    unistd::{
        setpgid,
        Pid,
    },
};
use request_handler::RequestHandler;
use rinit_ipc::Request;
use rinit_service::{
    dirs::Dirs,
    types::RunLevel,
};
use tokio::{
    fs,
    net::UnixListener,
    select,
    signal::unix::{
        signal,
        SignalKind,
    },
    sync::Mutex,
    task::{
        self,
        spawn_local,
        JoinError,
    },
};
use tracing::{
    debug,
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
    static ref SIGINT: Mutex<tokio::signal::unix::Signal> =
        Mutex::new(signal(SignalKind::interrupt()).unwrap());
    static ref SIGTERM: Mutex<tokio::signal::unix::Signal> =
        Mutex::new(signal(SignalKind::terminate()).unwrap());
}

pub async fn signal_wait() -> Signal {
    let mut sigint = SIGINT.lock().await;
    let mut sigterm = SIGTERM.lock().await;
    select! {
        _ = sigint.recv() => Signal::SIGINT,
        _ = sigterm.recv() => Signal::SIGTERM,
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = parse_args()?;
    let config = Dirs::new(args.config)?;

    // Setup logging
    let (file_writer, _fw_handle) = FileLogWriter::builder(
        FileSpec::default()
            .directory(&config.logdir)
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

    // Create its own process group
    setpgid(Pid::from_raw(0), Pid::from_raw(0))?;

    let local = task::LocalSet::new();
    let live_graph = LiveServiceGraph::new(config)?;

    // Setup socket listener
    fs::create_dir_all(Path::new(rinit_ipc::get_host_address()).parent().unwrap())
        .await
        .unwrap();

    let listener = Rc::new(
        UnixListener::bind(rinit_ipc::get_host_address()).with_context(|| {
            format!(
                "rinit is already running or didn't exit properly. Delete {:?} if needed",
                rinit_ipc::get_host_address()
            )
        })?,
    );
    local
        .run_until(async move {
            info!("Starting rinit.");
            let handler = Rc::new(RequestHandler::new(live_graph));
            let handles = Rc::new(RefCell::new(Vec::new()));

            let handler_clone = handler.clone();
            let handles_clone = handles.clone();
            let listener_clone = listener.clone();
            let request_handler_future = spawn_local(async move {
                let handler = handler_clone;
                let handles = handles_clone;
                let listener = listener_clone;
                loop {
                    let conn = listener.accept().await;
                    let stream = match conn {
                        Ok((stream, _addr)) => stream,
                        Err(err) => {
                            error!("error while accepting a new connection: {err}");
                            return;
                        }
                    };
                    let handler = handler.clone();
                    handles.borrow_mut().push(task::spawn_local(async move {
                        if let Err(err) = handler.handle_stream(stream, false).await {
                            error!("{err}");
                        }
                    }));
                }
            });

            info!("Starting boot services.");
            if let Err(err) = handler
                .handle_request(Request::StartAllServices(RunLevel::Boot))
                .await
            {
                error!("{err}");
            }

            info!("Starting services.");
            if let Err(err) = handler
                .handle_request(Request::StartAllServices(RunLevel::Default))
                .await
            {
                error!("{err}");
            }

            info!("Startup completed.");

            let signal = signal_wait().await;
            debug!("received signal {signal}");

            // Stop listening for requests by cancelling the future
            drop(request_handler_future);

            let handler_clone = handler.clone();
            let handles_clone = handles.clone();
            let request_handler_future = spawn_local(async move {
                let handler = handler_clone;
                let handles = handles_clone;
                loop {
                    let conn = listener.accept().await;
                    let stream = match conn {
                        Ok((stream, _addr)) => stream,
                        Err(err) => {
                            error!("error while accepting a new connection: {err}");
                            return;
                        }
                    };
                    let handler = handler.clone();
                    handles.borrow_mut().push(task::spawn_local(async move {
                        if let Err(err) = handler.handle_stream(stream, true).await {
                            error!("{err}");
                        }
                    }));
                }
            });

            // while let Some(handle) = handles.borrow_mut().pop() {
            //     let res: Result<(), JoinError> = handle.await;
            //     res.unwrap();
            // }
            debug!("awaited on all futures");

            info!("Stopping services");
            if let Err(err) = handler
                .handle_request(Request::StopAllServices(RunLevel::Default))
                .await
            {
                error!("{err}");
            }

            info!("Stopping boot services");
            if let Err(err) = handler
                .handle_request(Request::StopAllServices(RunLevel::Boot))
                .await
            {
                error!("{err}");
            }
            info!("Service shutdown completed.");
            while let Some(handle) = handles.borrow_mut().pop() {
                let res: Result<(), JoinError> = handle.await;
                res.unwrap();
            }
            debug!("awaited on all futures");

            drop(request_handler_future);
        })
        .await;

    fs::remove_file(rinit_ipc::get_host_address())
        .await
        .unwrap();

    Ok(())
}
