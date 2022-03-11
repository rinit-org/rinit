#![feature(async_closure)]
#![feature(new_uninit)]

mod live_service;
mod live_service_graph;

use std::{
    fs,
    sync::Arc,
};

use anyhow::Result;
use kansei_core::config::Config;
use kansei_exec::signal_wait;
use live_service_graph::LiveServiceGraph;
use tokio::{
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
            &fs::read(&*CONFIG.try_read().unwrap().get_graph_filename()).unwrap()
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

    loop {
        select! {
            _ = signal_wait()() => {
                break;
            }
        }
    }

    Ok(())
}
