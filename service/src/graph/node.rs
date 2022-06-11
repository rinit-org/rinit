use std::collections::{
    HashMap,
    HashSet,
};

use serde::{
    Deserialize,
    Serialize,
};

use crate::types::{
    Provider,
    Service,
};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Node {
    pub service: Service,
    pub dependents: HashSet<usize>,
    providers: HashMap<usize, Provider>,
}

impl Node {
    pub fn new(service: Service) -> Self {
        Node {
            service,
            dependents: HashSet::new(),
            providers: HashMap::new(),
        }
    }

    pub fn name(&self) -> &str {
        self.service.name()
    }

    pub fn add_dependent(
        &mut self,
        dependent: usize,
    ) {
        if !self.dependents.contains(&dependent) {
            self.dependents.insert(dependent);
        }
    }

    pub fn remove_dependent(
        &mut self,
        dependent: usize,
    ) {
        self.dependents.remove(&dependent);
    }

    pub fn has_dependents(&self) -> bool {
        !self.dependents.is_empty()
    }
}
