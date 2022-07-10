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
    #[serde(flatten)]
    pub service: Service,
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub dependents: HashSet<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    providers: HashMap<String, Provider>,
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
        dependent: String,
    ) {
        self.dependents.insert(dependent);
    }

    pub fn remove_dependent(
        &mut self,
        dependent: &str,
    ) {
        self.dependents.remove(dependent);
    }

    pub fn has_dependents(&self) -> bool {
        !self.dependents.is_empty()
    }
}
