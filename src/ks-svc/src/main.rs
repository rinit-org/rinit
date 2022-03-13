#![feature(async_closure)]
#![feature(new_uninit)]

mod live_service;
mod live_service_graph;
mod message_handler;

use std::{
    path::Path,
    sync::Arc,
};

use anyhow::Result;
use kansei_core::config::Config;
use kansei_exec::signal_wait;
use live_service_graph::LiveServiceGraph;
use message_handler::MessageHandler;
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

#[tokio::main]
async fn main() -> Result<()> {
    let config = Arc::new(Config::new(None)?);

    *CONFIG.write().await = config;

    tokio::spawn(async move {
        LIVE_GRAPH.start_all_services().await;
    });

    // Setup socket listener
    fs::create_dir_all(
        Path::new(kansei_message::get_host_address())
            .parent()
            .unwrap(),
    )
    .await
    .unwrap();
    let listener = UnixListener::bind(kansei_message::get_host_address()).unwrap();

    select! {
        _ = listen(listener) => {}
        _ = signal_wait() => {
            LIVE_GRAPH.stop_all_services().await;
        }
    }

    Ok(())
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
