#![feature(async_closure)]
#![feature(let_chains)]

pub mod exec_script;
pub mod kill_process;
pub mod pidfd_send_signal;
pub mod run_short_lived_script;
pub mod signal_wait;
pub mod supervise_long_lived_process;
pub mod supervise_short_lived_process;

pub use exec_script::exec_script;
pub use kill_process::kill_process;
pub use pidfd_send_signal::pidfd_send_signal;
use rinit_ipc::Message;
pub use run_short_lived_script::run_short_lived_script;
pub use signal_wait::signal_wait;
pub use supervise_long_lived_process::supervise_long_lived_process;
pub use supervise_short_lived_process::supervise_short_lived_process;
use tracing::info;
pub mod live_service;
pub mod live_service_graph;
pub mod message_handler;

use std::{
    io,
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
use clap::{
    Parser,
    Subcommand,
};
use live_service_graph::LiveServiceGraph;
use message_handler::MessageHandler;
use rinit_service::config::Config;
use tokio::{
    fs,
    io::AsyncWriteExt,
    join,
    net::{
        UnixListener,
        UnixStream,
    },
    select,
    task::{
        self,
        JoinError,
    },
};

#[macro_use]
extern crate lazy_static;

#[derive(Parser)]
struct Opts {
    #[clap(subcommand)]
    subcmd: Subcmd,
}

#[derive(Subcommand)]
enum Subcmd {
    Run { config_path: Option<PathBuf> },
    Oneshot { phase: String, service: String },
    Longrun { service: String },
}

fn syscall_result(ret: libc::c_long) -> io::Result<libc::c_long> {
    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}

pub async fn service_control(config: Config) -> Result<()> {
    install_tracing();

    let local = task::LocalSet::new();
    // Setup socket listener
    fs::create_dir_all(Path::new(rinit_ipc::get_host_address()).parent().unwrap())
        .await
        .unwrap();
    let live_graph = LiveServiceGraph::new(config).unwrap();

    local
        .run_until(async move {
            let listener = UnixListener::bind(rinit_ipc::get_host_address()).unwrap();
            let handler = Rc::new(MessageHandler::new(live_graph));
            let mut handles = Vec::new();
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
                        let (stream, _addr) = conn.unwrap();
                        let handler = handler.clone();
                        handles.push(task::spawn_local(async move {
                            handler.handle_stream(stream).await;
                        }));
                    }
                }
            }
        })
        .await;

    fs::remove_file(get_host_address()).await.unwrap();

    Ok(())
}

fn install_tracing() {
    use tracing_error::ErrorLayer;
    use tracing_subscriber::{
        fmt,
        prelude::*,
        EnvFilter,
    };

    let fmt_layer = fmt::layer().with_target(false);
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .with(ErrorLayer::default())
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Opts::parse();
    match opts.subcmd {
        Subcmd::Run { config_path } => {
            service_control(Config::new(config_path).unwrap())
                .await
                .context("")?
        }
        Subcmd::Oneshot { phase, service } => {
            supervise_short_lived_process(&phase, &service)
                .await
                .context("")?
        }
        Subcmd::Longrun { service } => supervise_long_lived_process(&service).await.context("")?,
    }

    Ok(())
}
