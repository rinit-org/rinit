#![feature(async_closure)]

mod exec_args;
mod live_service;
mod live_service_graph;
mod oneshot_runner;
mod script_runner;

use std::{
    env,
    fs,
};

use anyhow::{
    Context,
    Result,
};
use async_std::task;
use kansei_core::{
    config::Config,
    graph::DependencyGraph,
};
use live_service_graph::LiveServiceGraph;

#[async_std::main]
async fn main() -> Result<()> {
    let config = Config::new(None)?;

    let graph_file = config.datadir.unwrap().join("graph.data");
    let graph: DependencyGraph = bincode::deserialize(
        &fs::read(&graph_file)
            .with_context(|| format!("unable to read graph from file {:?}", graph_file))?[..],
    )
    .with_context(|| format!("unable to deserialize graph from file {:?}", graph_file))?;

    let live_graph = LiveServiceGraph::new(graph)?;
    task::spawn(async move {
        live_graph.start_all_services();
    });

    loop {
        break;
    }

    Ok(())
}
