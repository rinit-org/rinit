#![feature(async_closure)]

pub mod live_service;
pub mod live_service_graph;
pub mod request_handler;
pub mod supervision;

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
use rinit_service::config::Config;
use tokio::{
    fs,
    join,
    net::UnixListener,
    select,
    signal::unix::{
        signal,
        SignalKind,
    },
    sync::{
        mpsc,
        watch,
        Mutex,
    },
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
};
use tracing_subscriber::{
    filter::LevelFilter,
    FmtSubscriber,
};

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
    let config = Config::new(args.config)?;

    // Setup logging
    let (file_writer, _fw_handle) = FileLogWriter::builder(
        FileSpec::default()
            .directory(&config.dirs.logdir)
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

    let (tx, mut rx) = mpsc::channel::<Request>(20);
    let local = task::LocalSet::new();
    let live_graph = LiveServiceGraph::new(config, tx.clone())?;

    // Setup socket listener
    fs::create_dir_all(Path::new(rinit_ipc::get_host_address()).parent().unwrap())
        .await
        .unwrap();

    let listener = UnixListener::bind(rinit_ipc::get_host_address()).with_context(|| {
        format!(
            "rinit is already running or didn't exit properly. Delete {:?} if needed",
            rinit_ipc::get_host_address()
        )
    })?;

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let mut shutdown = shutdown_tx.subscribe();
    let handler = Rc::new(RequestHandler::new(live_graph, shutdown_tx));
    let handles = Rc::new(RefCell::new(Vec::new()));
    local
        .run_until(async move {
            info!("Starting rinit.");

            let handler_clone = handler.clone();
            let handles_clone = handles.clone();
            let ipc_handler_future = spawn_local(async move {
                let handler = handler_clone;
                let handles = handles_clone;
                loop {
                    // Accept the connection here, it is simpler than letting RequestHandler do that
                    let conn = select! {
                        res = listener.accept() => {
                            res
                        },
                        _ = shutdown.changed() => {
                            break;
                        }
                    };
                    let stream = match conn {
                        Ok((stream, _addr)) => stream,
                        Err(err) => {
                            error!("error while accepting a new connection: {err}");
                            return;
                        }
                    };
                    let handler = handler.clone();
                    handles.borrow_mut().push(task::spawn_local(async move {
                        if let Err(err) = handler.handle_ipc_stream(stream).await {
                            error!("{err}");
                        }
                    }));
                }
            });

            let handler_clone = handler.clone();
            let handles_clone = handles.clone();
            let events_future = spawn_local(async move {
                let handler = handler_clone;
                let handles = handles_clone;
                loop {
                    let request = match rx.recv().await {
                        Some(req) => req,
                        None => break,
                    };
                    let handler = handler.clone();
                    if let Request::StopAllServices = request {
                        if let Err(err) = handler.handle_request(request).await {
                            error!("{err}");
                        }
                        break;
                    }
                    handles.borrow_mut().push(task::spawn_local(async move {
                        if let Err(err) = handler.handle_request(request).await {
                            error!("{err}");
                        }
                    }));
                }
            });

            // Starting rinit consists of 2 different phases
            // The first one is starting the boot services, the second one starts all the
            // other services
            if let Err(err) = tx.send(Request::StartAllServices).await {
                error!("{err}");
            }

            let (res1, res2, _) = join! {
                ipc_handler_future,
                events_future,
                async {
                    let signal = select! {
                        signal = signal_wait() => {
                            Some(signal)
                        },
                        _ = shutdown_rx.changed() => {
                            None
                        }
                    };
                    if let Some(signal) = signal  {
                        debug!("received signal {signal}");
                        if let Err(err) = tx.send(Request::StopAllServices).await {
                            error!("{err}");
                        }
                    }
                }
            };
            res1.unwrap();
            res2.unwrap();

            while let Some(handle) = handles.borrow_mut().pop() {
                let res: Result<(), JoinError> = handle.await;
                res.unwrap();
            }
            debug!("awaited on all futures");
        })
        .await;

    fs::remove_file(rinit_ipc::get_host_address())
        .await
        .unwrap();

    Ok(())
}
