use std::{
    fs,
    io,
    path::{
        Path,
        PathBuf,
    },
};

use clap::Parser;
use kansei_core::graph::{
    DependencyGraph,
    DependencyGraphError,
};
use kansei_parser::{
    parse_services,
    ServicesParserError,
};
use snafu::{
    ResultExt,
    Snafu,
};

use crate::Config;

#[derive(Parser)]
pub struct EnableCommand {
    services: Vec<String>,
}

#[derive(Snafu, Debug)]
pub enum EnableCommandError {
    #[snafu(display("could not read file {:?}: {}", file, source))]
    FileRead {
        file: PathBuf,
        source: io::Error,
    },
    #[snafu(display("could not deserialize graph: {}", source))]
    GraphDeserialize {
        source: bincode::Error,
    },
    ParseServicesError {
        source: ServicesParserError,
    },
    GraphError {
        source: DependencyGraphError,
    },
    GraphSerialize {
        source: bincode::Error,
    },
}

unsafe impl Send for EnableCommandError {}
unsafe impl Sync for EnableCommandError {}

impl EnableCommand {
    pub async fn run(
        self,
        config: Config,
    ) -> Result<(), EnableCommandError> {
        let graph_file = config.get_graph_filename();
        let mut graph: DependencyGraph = if graph_file.exists() {
            bincode::deserialize(
                &fs::read(&graph_file).with_context(|| {
                    FileRead {
                        file: graph_file.clone(),
                    }
                })?[..],
            )
            .context(GraphDeserialize {})?
        } else {
            DependencyGraph::new()
        };
        let services = parse_services(
            self.services.clone(),
            &config.service_directories,
            config.system,
        )
        .await
        .context(ParseServicesError {})?;
        graph
            .add_services(self.services, services)
            .context(GraphError {})?;

        fs::create_dir_all(graph_file.parent().unwrap());
        fs::write(
            graph_file,
            bincode::serialize(&graph).context(GraphSerialize)?,
        );

        Ok(())
    }
}
