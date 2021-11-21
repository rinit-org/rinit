use std::fs;

use anyhow::{
    ensure,
    Context,
    Result,
};
use clap::Parser;
use kansei_core::graph::DependencyGraph;
use kansei_parser::parse_services;

use crate::Config;

#[derive(Parser)]
pub struct EnableCommand {
    services: Vec<String>,
}

impl EnableCommand {
    pub async fn run(
        self,
        config: Config,
    ) -> Result<()> {
        // TODO: Print duplicated service
        ensure!(
            !(1..self.services.len()).any(|i| self.services[i..].contains(&self.services[i - 1])),
            "duplicated service found"
        );
        let graph_file = config.get_graph_filename();
        let mut graph: DependencyGraph = if graph_file.exists() {
            bincode::deserialize(
                &fs::read(&graph_file).with_context(|| format!("unable to read graph from file {:?}", graph_file)
                )?[..],
            )
            .context("unable to deserialize the dependency graph")?
        } else {
            DependencyGraph::new()
        };
        let services = parse_services(
            self.services.clone(),
            &config.service_directories,
            config.system,
        )
        .await
        .context("unable to parse services")?;
        graph
            .add_services(self.services, services)
            .context("unable to add the parsed services to the dependency graph")?;

        fs::create_dir_all(graph_file.parent().unwrap()).with_context(|| {
            format!("unable to create parent directory of file {:?}", graph_file)
        })?;
        fs::write(
            &graph_file,
            bincode::serialize(&graph).context("unable to serialize the dependency graph")?,
        )
        .with_context(|| format!("unable to write the dependency graph to {:?}", graph_file))?;

        Ok(())
    }
}
