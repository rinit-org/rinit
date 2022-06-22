use std::collections::{
    HashMap,
    HashSet,
};

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
    #[snafu(display("the dependency {} of service {} is missing", dependency, service))]
    DependenciesUnfulfilledError { service: String, dependency: String },
    #[snafu(display("found a cycle in the dependency graph"))]
    CycleFoundError,
    #[snafu(display("service {} is not enabled", service))]
    ServiceNotEnabled { service: String },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DependencyGraph {
    pub enabled_services: HashSet<usize>,
    nodes_index: HashMap<String, usize>,
    pub nodes: Vec<Node>,
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
            nodes_index: HashMap::new(),
            nodes: Vec::new(),
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
    pub fn add_services(
        &mut self,
        services_to_enable: Vec<String>,
        services: Vec<Service>,
    ) -> Result<()> {
        let (new_services, existing_services): (Vec<Service>, Vec<Service>) = services
            .into_iter()
            .partition(|service| self.get_index_from_name(service.name()).is_none());

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
            let index = *self.get_index_from_name(service).unwrap();
            self.enabled_services.insert(index);
            let dependencies = self.nodes[index].service.dependencies().to_owned();
            for dep in dependencies {
                self.get_node_from_name(&dep).add_dependent(index);
            }
        });

        self.check_cycles(
            services_to_enable
                .iter()
                .map(|name| *self.get_index_from_name(name).unwrap())
                .collect(),
        )?;

        Ok(())
    }

    fn add_nodes(
        &mut self,
        services: Vec<Service>,
    ) -> usize {
        let ret = self.nodes.len();
        let mut current_index = ret;
        self.nodes.reserve_exact(services.len());
        services.into_iter().for_each(|service| {
            self.nodes.push(Node::new(service));
            self.nodes_index
                .insert(self.nodes.last().unwrap().name().to_string(), current_index);
            current_index += 1;
        });

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
                let service_index = *self.nodes_index.get(new_service.name()).unwrap();
                let existing_service = &self.nodes.get(service_index).unwrap().service;

                if existing_service != &new_service {
                    return false;
                }

                // Remove all instances of this service from Node::dependents
                let dependencies = existing_service.dependencies().to_owned();
                for dep in dependencies {
                    self.get_node_from_name(dep.as_str())
                        .remove_dependent(service_index);
                }
                self.nodes.insert(service_index, Node::new(new_service));
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
            let node = self.nodes.get(*index).unwrap();
            node.service
                .dependencies()
                .to_owned()
                .iter()
                .for_each(|dep| self.get_node_from_name(dep).add_dependent(*index));
        });
    }

    fn check_dependencies(
        &self,
        from: usize,
    ) -> Result<()> {
        self.nodes[from..]
            .iter()
            .try_for_each(|node| -> Result<()> {
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
            .map(|node| {
                (
                    *self.get_index_from_name(node.name()).unwrap(),
                    Color::White,
                )
            })
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
            .get(node)
            .unwrap()
            .service
            .dependencies()
            .iter()
            .map(|dep| *self.get_index_from_name(dep).unwrap())
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

    pub fn remove_services(
        &mut self,
        services: Vec<String>,
    ) -> Result<()> {
        services.iter().try_for_each(|service| -> Result<()> {
            let node_index = *self
                .nodes_index
                .get(service)
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
                let dep_index = *self.get_index_from_name(dep).unwrap();
                self.nodes[dep_index].remove_dependent(index);
                if !self.is_node_required(dep_index) {
                    self.remove_node(dep_index)
                }
            });

        // The node to remove is the last one
        if index == self.nodes.len() - 1 {
            self.nodes.pop();
            self.nodes_index.remove(&name);
            return;
        }

        self.nodes[index] = self.nodes.pop().unwrap();
        self.nodes_index
            .insert(self.nodes[index].name().to_string(), index);
        self.nodes_index.remove(&name);
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
        self.nodes_index.contains_key(name)
    }

    #[inline]
    fn get_node_from_name(
        &mut self,
        name: &str,
    ) -> &mut Node {
        self.nodes
            .get_mut(*self.nodes_index.get(name).unwrap())
            .unwrap()
    }

    #[inline]
    fn get_index_from_name(
        &self,
        name: &str,
    ) -> Option<&usize> {
        self.nodes_index.get(name)
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
    fn remove_service() {
        let mut graph = DependencyGraph::new();

        graph
            .add_services(
                vec!["foo".to_string()],
                vec![create_new_service("foo", ServiceOptions::new())],
            )
            .unwrap();

        graph.remove_services(vec!["foo".to_string()]).unwrap();
        assert_eq!(graph.nodes.len(), 0);
    }

    #[test]
    fn remove_service_with_dependency() {
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

        graph.remove_services(vec!["foo".to_string()]).unwrap();
        assert_eq!(graph.nodes.len(), 0);
    }
}
