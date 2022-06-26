use std::fs;

use anyhow::{
    ensure,
    Context,
    Result,
};
use clap::Parser;
use rinit_service::graph::DependencyGraph;

use crate::Config;

#[derive(Parser)]
pub struct DisableCommand {
    services: Vec<String>,
    #[clap(long = "atomic-changes")]
    pub atomic_changes: bool,
}

impl DisableCommand {
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
        ensure!(
            graph_file.exists(),
            "the graph has not been initialized yet"
        );
        let mut graph: DependencyGraph = serde_json::from_slice(
            &fs::read(&graph_file)
                .with_context(|| format!("unable to read graph from file {:?}", graph_file))?[..],
        )
        .context("unable to deserialize the dependency graph")?;
        if self.atomic_changes {
            graph
                .disable_services(self.services)
                .context("unable to remove services in the dependency graph")?;

            println!("All the services have been disabled.");
        } else {
            self.services
                .into_iter()
                .try_for_each(|service| -> Result<()> {
                    graph
                        .disable_services(vec![service.clone()])
                        .with_context(|| {
                            format!("unable to disable service {service} in the dependency graph")
                        })?;
                    println!("The service {service} has been disabled.");
                    Ok(())
                })?;
        }

        fs::write(
            &graph_file,
            serde_json::to_vec(&graph).context("unable to serialize the dependency graph")?,
        )
        .with_context(|| format!("unable to write the dependency graph to {:?}", graph_file))?;

        Ok(())
    }
}
