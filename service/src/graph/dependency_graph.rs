use std::collections::{
    HashMap,
    HashSet,
};

use indexmap::IndexMap;
use serde::{
    Deserialize,
    Serialize,
};
use snafu::{
    ensure,
    OptionExt,
    Snafu,
};

use crate::{
    graph::Node,
    types::Service,
};

#[derive(Debug, Snafu, PartialEq, Eq)]
pub enum DependencyGraphError {
    #[snafu(display("found a cycle in the dependency graph"))]
    CycleFoundError,
    #[snafu(display(
        "service {service} does not have the same runlevel as its dependency {dependency}"
    ))]
    DependenciesMustHaveSameRunLevel { service: String, dependency: String },
    #[snafu(display("the dependency {} of service {} is missing", dependency, service))]
    DependenciesUnfulfilledError { service: String, dependency: String },
    #[snafu(display("service {} is not enabled", service))]
    ServiceNotEnabled { service: String },
    #[snafu(display("service {service} is already enabled"))]
    ServiceAlreadyEnabled { service: String },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DependencyGraph {
    pub enabled_services: HashSet<usize>,
    pub nodes: IndexMap<String, Node>,
}

enum Color {
    White,
    Gray,
    Black,
}

impl DependencyGraph {
    pub fn new() -> Self {
        DependencyGraph {
            enabled_services: HashSet::new(),
            nodes: IndexMap::new(),
        }
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

type Result<T, E = DependencyGraphError> = std::result::Result<T, E>;

impl DependencyGraph {
    // services_to_enable nor services should have duplicates,
    // otherwise everything break
    pub fn add_services(
        &mut self,
        services_to_enable: Vec<String>,
        services: Vec<Service>,
    ) -> Result<()> {
        services_to_enable.iter().try_for_each(|service| {
            if let Some(index) = &self.nodes.get_index_of(service) {
                ensure!(
                    !self.enabled_services.contains(index),
                    ServiceAlreadyEnabledSnafu { service }
                );
            }
            Ok(())
        })?;

        // Split the services into two different vectors
        // One that contains services that the graph doesn't have
        // The other contains services that are already inserted in the graph
        // This way we can optimize the addition of services
        let (new_services, existing_services) = services
            .into_iter()
            .partition(|service| !self.nodes.contains_key(service.name()));

        let index = self.nodes.len();
        self.add_nodes(new_services);
        let starting_index = if self.replace_existing_nodes(existing_services) {
            0
        } else {
            index
        };

        self.check_dependencies(starting_index)?;

        // Update enabled services set and populate dependents
        services_to_enable.iter().for_each(|service| {
            let index = self.nodes.get_index_of(service).unwrap();
            self.enabled_services.insert(index);
            let dependencies = self.nodes[index].service.dependencies().to_owned();
            for dep in dependencies {
                self.nodes
                    .get_mut(&dep)
                    .unwrap()
                    .add_dependent(service.to_string());
            }
        });

        self.check_cycles(
            services_to_enable
                .iter()
                .map(|name| self.nodes.get_index_of(name).unwrap())
                .collect(),
        )?;

        Ok(())
    }

    fn add_nodes(
        &mut self,
        services: Vec<Service>,
    ) -> usize {
        let ret = self.nodes.len();
        self.nodes.reserve(services.len());
        for service in services {
            self.nodes
                .insert(service.name().to_string(), Node::new(service));
        }

        ret
    }

    // Get a vector of services that we know another instance of the same exists in
    // the graph If they are different, replace the old one with the new one
    // Return true when at last one has been modified
    fn replace_existing_nodes(
        &mut self,
        services: Vec<Service>,
    ) -> bool {
        services
            .into_iter()
            .map(|new_service| -> bool {
                let (service_index, name, node) = self.nodes.get_full(new_service.name()).unwrap();
                let existing_service = &node.service;
                if existing_service == &new_service {
                    return false;
                }

                let name = name.clone();
                // Remove all instances of this service from Node::dependents
                let dependencies = existing_service.dependencies().to_owned();
                for dep in dependencies {
                    self.nodes.get_mut(&dep).unwrap().remove_dependent(&name);
                }
                self.nodes
                    .insert(new_service.name().to_string(), Node::new(new_service));
                self.populate_dependents(&[service_index]);
                true
            })
            .any(|res| res)
    }

    fn populate_dependents(
        &mut self,
        services_index: &[usize],
    ) {
        services_index.iter().for_each(|index| {
            let (name, node) = self.nodes.get_index(*index).unwrap();
            let name = name.clone();
            node.service
                .dependencies()
                .to_owned()
                .iter()
                .for_each(|dep| self.nodes.get_mut(dep).unwrap().add_dependent(name.clone()));
        });
    }

    fn check_dependencies(
        &self,
        from: usize,
    ) -> Result<()> {
        self.nodes
            .values()
            .skip(from)
            .try_for_each(|node| -> Result<()> {
                let runlevel = node.service.runlevel();
                node.service
                    .dependencies()
                    .iter()
                    .try_for_each(|dep| -> Result<()> {
                        ensure!(
                            self.has_service(dep),
                            DependenciesUnfulfilledSnafu {
                                service: node.name().to_owned(),
                                dependency: dep.to_owned()
                            }
                        );
                        ensure!(
                            self.nodes.get(dep).unwrap().service.runlevel() == runlevel,
                            DependenciesMustHaveSameRunLevelSnafu {
                                service: node.name(),
                                dependency: dep
                            });
                        Ok(())
                    })
            })?;
        Ok(())
    }

    fn check_cycles(
        &self,
        services_to_enable: Vec<usize>,
    ) -> Result<()> {
        let mut colors: HashMap<usize, Color> = self
            .nodes
            .iter()
            .map(|(name, _node)| (self.nodes.get_index_of(name).unwrap(), Color::White))
            .collect();

        services_to_enable
            .iter()
            .try_for_each(|node| -> Result<()> { self.visit(&mut colors, *node) })?;

        Ok(())
    }

    fn visit(
        &self,
        colors: &mut HashMap<usize, Color>,
        node: usize,
    ) -> Result<()> {
        colors.insert(node, Color::Gray);

        self.nodes
            .get_index(node)
            .unwrap()
            .1
            .service
            .dependencies()
            .iter()
            .map(|dep| self.nodes.get_index_of(dep).unwrap())
            .try_for_each(|dep| -> Result<()> {
                match colors.get(&dep).unwrap() {
                    Color::White => self.visit(colors, dep),
                    Color::Gray => Err(DependencyGraphError::CycleFoundError {}),
                    Color::Black => Ok(()),
                }
            })?;

        colors.insert(node, Color::Black);
        Ok(())
    }

    pub fn disable_services(
        &mut self,
        services: Vec<String>,
    ) -> Result<()> {
        services.iter().try_for_each(|service| -> Result<()> {
            let node_index = self
                .nodes
                .get_index_of(service)
                .context(ServiceNotEnabledSnafu { service })?;
            self.enabled_services.remove(&node_index);
            if !self.is_node_required(node_index) {
                self.remove_node(node_index);
            }

            Ok(())
        })
    }

    fn remove_node(
        &mut self,
        index: usize,
    ) {
        let name = self.nodes[index].name().to_owned();
        // This node has already been removed from the graph
        if !self.has_service(&name) {
            return;
        }

        self.nodes[index]
            .service
            .dependencies()
            .to_owned()
            .iter()
            .for_each(|dep| {
                let dep_index = self.nodes.get_index_of(dep).unwrap();
                self.nodes[dep_index].remove_dependent(&name);
                if !self.is_node_required(dep_index) {
                    self.remove_node(dep_index)
                }
            });

        // The node to remove is the last one
        if index == self.nodes.len() - 1 {
            self.nodes.pop();
            return;
        }

        self.nodes[index] = self.nodes.pop().unwrap().1;
    }

    fn is_node_required(
        &self,
        index: usize,
    ) -> bool {
        self.enabled_services.contains(&index) || self.nodes[index].has_dependents()
    }

    #[inline]
    fn has_service(
        &self,
        name: &str,
    ) -> bool {
        self.nodes.contains_key(name)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::types::*;

    fn create_new_service(
        name: &str,
        options: ServiceOptions,
    ) -> Service {
        Service::Oneshot(Oneshot {
            name: name.to_string(),
            start: Script::new(ScriptPrefix::Bash, "exit 0".to_string()),
            stop: None,
            options,
        })
    }

    #[test]
    fn add_services_to_empty_graph() {
        let mut graph = DependencyGraph::new();

        graph
            .add_services(
                vec!["foo".to_string()],
                vec![create_new_service("foo", ServiceOptions::new())],
            )
            .unwrap();
        assert_eq!(graph.nodes.len(), 1);
    }

    #[test]
    fn add_service_with_dependency() {
        let mut graph = DependencyGraph::new();

        graph
            .add_services(
                vec!["foo".to_string()],
                vec![
                    create_new_service("foo", {
                        let mut options = ServiceOptions::new();
                        options.dependencies = vec!["bar".to_string()];
                        options
                    }),
                    create_new_service("bar", ServiceOptions::new()),
                ],
            )
            .unwrap();
        assert_eq!(graph.nodes.len(), 2);
    }

    #[test]
    fn add_service_with_multiple_dependencies() {
        let mut graph = DependencyGraph::new();

        graph
            .add_services(
                vec!["foobar".to_string()],
                vec![
                    create_new_service("foobar", {
                        let mut options = ServiceOptions::new();
                        options.dependencies = vec!["bar".to_string(), "foo".to_string()];
                        options
                    }),
                    create_new_service("bar", ServiceOptions::new()),
                    create_new_service("foo", ServiceOptions::new()),
                ],
            )
            .unwrap();
        assert_eq!(graph.nodes.len(), 3);
    }

    #[test]
    fn add_service_with_duplicated_services() {
        let mut graph = DependencyGraph::new();

        graph
            .add_services(
                vec!["foo".to_string()],
                vec![create_new_service("foo", ServiceOptions::new())],
            )
            .unwrap();

        graph
            .add_services(
                vec!["bar".to_string()],
                vec![
                    create_new_service("foo", ServiceOptions::new()),
                    create_new_service("bar", {
                        let mut options = ServiceOptions::new();
                        options.dependencies = vec!["foo".to_string()];
                        options
                    }),
                ],
            )
            .unwrap();
        assert_eq!(graph.nodes.len(), 2);
    }

    #[test]
    fn add_service_with_unfulfilled_dependency() {
        let mut graph = DependencyGraph::new();

        let res = graph.add_services(
            vec!["foo".to_string()],
            vec![create_new_service("foo", {
                let mut options = ServiceOptions::new();
                options.dependencies = vec!["bar".to_string()];
                options
            })],
        );
        assert!(res.is_err());
        assert_eq!(
            res,
            Err(DependencyGraphError::DependenciesUnfulfilledError {
                dependency: "bar".to_string(),
                service: "foo".to_string()
            })
        );
    }

    #[test]
    fn add_services_with_cycle() {
        let mut graph = DependencyGraph::new();

        let res = graph.add_services(
            vec!["foo".to_string()],
            vec![
                create_new_service("foo", {
                    let mut options = ServiceOptions::new();
                    options.dependencies = vec!["bar".to_string()];
                    options
                }),
                create_new_service("bar", {
                    let mut options = ServiceOptions::new();
                    options.dependencies = vec!["foo".to_string()];
                    options
                }),
            ],
        );

        assert!(res.is_err());
        assert_eq!(res, Err(DependencyGraphError::CycleFoundError));
    }

    #[test]
    fn disable_service() {
        let mut graph = DependencyGraph::new();

        graph
            .add_services(
                vec!["foo".to_string()],
                vec![create_new_service("foo", ServiceOptions::new())],
            )
            .unwrap();

        graph.disable_services(vec!["foo".to_string()]).unwrap();
        assert_eq!(graph.nodes.len(), 0);
    }

    #[test]
    fn disable_service_with_dependency() {
        let mut graph = DependencyGraph::new();

        graph
            .add_services(
                vec!["foo".to_string()],
                vec![
                    create_new_service("foo", {
                        let mut options = ServiceOptions::new();
                        options.dependencies = vec!["bar".to_string()];
                        options
                    }),
                    create_new_service("bar", ServiceOptions::new()),
                ],
            )
            .unwrap();

        graph.disable_services(vec!["foo".to_string()]).unwrap();
        assert_eq!(graph.nodes.len(), 0);
    }

    #[test]
    fn services_with_different_runlevel() {
        let mut graph = DependencyGraph::new();

        assert_eq!(
            graph
                .add_services(
                    vec!["foo".to_string()],
                    vec![
                        create_new_service("foo", {
                            let mut options = ServiceOptions::new();
                            options.dependencies = vec!["bar".to_string()];
                            options
                        }),
                        create_new_service("bar", {
                            use crate::types::RunLevel;
                            let mut options = ServiceOptions::new();
                            options.runlevel = RunLevel::Boot;
                            options
                        }),
                    ],
                )
                .unwrap_err(),
            DependencyGraphError::DependenciesMustHaveSameRunLevel {
                service: "foo".to_string(),
                dependency: "bar".to_string()
            }
        );
    }
}
