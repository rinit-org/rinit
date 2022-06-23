use serde::{
    Deserialize,
    Serialize,
};

/// Store options for Longrun and Oneshot
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct ServiceOptions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires_one: Vec<String>,
    pub auto_start: bool,
}

impl ServiceOptions {
    pub fn new() -> ServiceOptions {
        ServiceOptions {
            dependencies: Vec::new(),
            requires: Vec::new(),
            requires_one: Vec::new(),
            auto_start: true,
        }
    }
}
impl Default for ServiceOptions {
    fn default() -> Self {
        Self::new()
    }
}
