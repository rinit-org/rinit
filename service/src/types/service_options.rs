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
    #[serde(
        default = "ServiceOptions::default_autostart",
        skip_serializing_if = "ServiceOptions::is_default_autostart"
    )]
    pub autostart: bool,
}

impl ServiceOptions {
    pub fn new() -> ServiceOptions {
        ServiceOptions {
            dependencies: Vec::new(),
            requires: Vec::new(),
            requires_one: Vec::new(),
            autostart: Self::default_autostart(),
        }
    }

    const fn default_autostart() -> bool {
        true
    }

    fn is_default_autostart(autostart: &bool) -> bool {
        *autostart
    }
}
impl Default for ServiceOptions {
    fn default() -> Self {
        Self::new()
    }
}
