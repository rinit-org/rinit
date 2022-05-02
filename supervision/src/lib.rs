#![feature(async_closure)]
#![feature(new_uninit)]

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
pub use run_short_lived_script::run_short_lived_script;
pub use signal_wait::signal_wait;

pub mod live_service;
pub mod live_service_graph;
pub mod message_handler;

use std::{
    io,
    path::Path,
    sync::Arc,
};

use anyhow::Result;
use live_service_graph::LiveServiceGraph;
use message_handler::MessageHandler;
use rinit_service::config::Config;
use tokio::{
    fs,
    net::UnixListener,
    select,
    sync::RwLock,
};

#[macro_use]
extern crate lazy_static;

lazy_static! {
    pub static ref CONFIG: RwLock<Arc<Config>> =
        RwLock::new(unsafe { Arc::new_zeroed().assume_init() });
    pub static ref LIVE_GRAPH: LiveServiceGraph = LiveServiceGraph::new(
        serde_json::from_slice(
            &std::fs::read(&*CONFIG.try_read().unwrap().get_graph_filename()).unwrap()
        )
        .unwrap()
    )
    .unwrap();
}

fn syscall_result(ret: libc::c_long) -> io::Result<libc::c_long> {
    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}

pub async fn service_control(config: Config) -> Result<()> {
    // install_tracing();
    let config = Arc::new(config);

    *CONFIG.write().await = config;

    tokio::spawn(async move {
        LIVE_GRAPH.start_all_services().await;
    });

    // Setup socket listener
    fs::create_dir_all(Path::new(rinit_ipc::get_host_address()).parent().unwrap())
        .await
        .unwrap();
    let listener = UnixListener::bind(rinit_ipc::get_host_address()).unwrap();

    select! {
        _ = listen(listener) => {}
        _ = signal_wait() => {
            LIVE_GRAPH.stop_all_services().await;
        }
    }

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

async fn listen(listener: UnixListener) -> ! {
    loop {
        let conn = listener.accept().await.unwrap();
        let (stream, _addr) = conn;
        tokio::spawn(async {
            let handler = MessageHandler::new(&LIVE_GRAPH);
            handler.handle(stream).await;
        });
    }
}
