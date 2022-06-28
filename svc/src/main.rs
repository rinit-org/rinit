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
use lexopt::prelude::{
    Long,
    Short,
};
use live_service_graph::LiveServiceGraph;
use request_handler::RequestHandler;
use rinit_ipc::Request;
use rinit_service::config::Config;
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

#[macro_use]
extern crate lazy_static;

fn parse_config_file() -> Result<Option<PathBuf>, lexopt::Error> {
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
    let config_file = parse_config_file()?;
    let config = Config::new(config_file)?;

    // Setup logging
    use tracing_error::ErrorLayer;
    use tracing_subscriber::{
        fmt,
        prelude::*,
        EnvFilter,
    };

    let file_appender =
        tracing_appender::rolling::daily(config.logdir.as_ref().unwrap(), "rinit.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    let fmt_layer = fmt::layer().with_target(false);
    let file_fmt_layer = fmt::layer().with_target(false).with_writer(non_blocking);
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .with(file_fmt_layer)
        .with(ErrorLayer::default())
        .init();

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
                if let Err(err) = handler_clone.handle(Request::StartAllServices).await {
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
