use std::fs;

use anyhow::{
    bail,
    ensure,
    Context,
    Result,
};
use clap::Parser;
use rinit_ipc::{
    AsyncConnection,
    Request,
};
use rinit_parser::parse_services;
use rinit_service::{
    graph::DependencyGraph,
    types::RunLevel,
};

use crate::{
    util::start_service,
    Dirs,
};

#[derive(Parser)]
pub struct EnableCommand {
    #[clap(required = true)]
    services: Vec<String>,
    #[clap(long = "atomic-changes")]
    pub atomic_changes: bool,
    #[clap(short, long = "start")]
    pub start: bool,
    #[clap(long, default_value_t)]
    runlevel: RunLevel,
    #[clap(long)]
    stop_at_errors: bool,
}

impl EnableCommand {
    pub async fn run(
        self,
        dirs: Dirs,
    ) -> Result<()> {
        // TODO: Print duplicated service
        ensure!(
            !(1..self.services.len()).any(|i| self.services[i..].contains(&self.services[i - 1])),
            "duplicated service found"
        );
        let graph_file = dirs.graph_filename();
        let mut graph: DependencyGraph = if graph_file.exists() {
            serde_json::from_slice(
                &fs::read(&graph_file).with_context(|| format!("unable to read graph from file {:?}", graph_file)
                )?[..],
            )
            .context("unable to deserialize the dependency graph")?
        } else {
            DependencyGraph::new()
        };

        let uid = unsafe { libc::getuid() };
        let system_mode = uid == 0;

        let save_graph = |graph: &DependencyGraph| -> Result<()> {
            fs::create_dir_all(graph_file.parent().unwrap()).with_context(|| {
                format!("unable to create parent directory of file {:?}", graph_file)
            })?;
            fs::write(
                &graph_file,
                serde_json::to_vec(&graph).context("unable to serialize the dependency graph")?,
            )
            .with_context(|| format!("unable to write the dependency graph to {:?}", graph_file))?;

            Ok(())
        };

        let mut success = true;
        if self.atomic_changes {
            let services = parse_services(self.services.clone(), &dirs, system_mode)
                .context("unable to parse services")?;
            // The dependency graph ensure that all the dependencies have the same runlevel
            // So we just check that we the services passed on the command line are the
            // same runlevel requested
            ensure!(
                services
                    .iter()
                    .filter(|service| self.services.contains(&service.name().to_string()))
                    .all(|service| service.runlevel() == self.runlevel),
                "service {} must be of the runlevel {:?}",
                services
                    .iter()
                    .filter(|service| self.services.contains(&service.name().to_string()))
                    .find(|service| service.runlevel() != self.runlevel)
                    .unwrap()
                    .name(),
                self.runlevel
            );
            graph
                .add_services(self.services.clone(), services)
                .context("unable to add the parsed services to the dependency graph")?;
            save_graph(&graph)?;
            println!("All the services have been enabled.");
            // In this case we have enabled all services at once
            // Ask for a graph reload
            if let Ok(mut conn) = AsyncConnection::new_host_address().await {
                let request = Request::ReloadGraph;
                conn.send_request(request).await??;

                // If the user asked us to start the services, try to start them one by one
                if self.start {
                    for service in &self.services {
                        if start_service(&mut conn, service, self.runlevel).await? {
                            println!("Service {service} started successfully.");
                        } else {
                            println!("Service {service} failed to start.");
                            success = false;
                        }
                    }
                }
            } else {
                // We couldn't connect. In case --start has been passed, this is considered an
                // error
                ensure!(
                    !self.start,
                    "Could not start services because we couldn't connect ot the service control \
                     daemon"
                )
            }
        } else {
            let mut conn = if let Ok(conn) = AsyncConnection::new_host_address().await {
                Some(conn)
            } else {
                if self.start {
                    eprintln!(
                        "Could not connect to the service control daemon, services won't be \
                         started"
                    )
                }
                None
            };

            let add_service = |service: &str, graph: &mut DependencyGraph| -> Result<()> {
                let services = parse_services(vec![service.to_owned()], &dirs, system_mode)
                    .with_context(|| {
                        format!("unable to parse service {service} and its dependencies")
                    })?;
                ensure!(
                    services
                        .iter()
                        .find(|s| service == s.name())
                        .unwrap()
                        .runlevel()
                        == self.runlevel,
                    "service {service} must be of the runlevel {:?}",
                    self.runlevel
                );
                graph
                    .add_services(vec![service.to_owned()], services)
                    .with_context(|| {
                        format!(
                            "unable to add service {service} and its dependencies to the \
                             dependency graph"
                        )
                    })?;

                Ok(())
            };
            for service in self.services {
                let res = add_service(&service, &mut graph)
                    .with_context(|| format!("Could not enable service {service}"));
                if let Err(err) = res {
                    if self.stop_at_errors {
                        bail!(err);
                    } else {
                        eprintln!("err");
                        success = false;
                    }
                }
                // Always save the graph. We save after each service, so that in case of any
                // error, we have already it saved to disk and we can exit this function
                save_graph(&graph)?;
                println!("Service {service} has been enabled");
                if let Some(conn) = &mut conn {
                    let request = Request::ReloadGraph;
                    conn.send_request(request).await??;

                    if self.start {
                        let res = start_service(conn, &service, self.runlevel).await;
                        if self.stop_at_errors {
                            res.with_context(|| format!("Could not start service {service}"))?;
                        } else {
                            println!("Service {service} failed to start.");
                            success = false;
                        }
                        println!("Service {service} started successfully.");
                    }
                }
            }
        }

        ensure!(success, "Could not complete the operation successfully");

        Ok(())
    }
}
