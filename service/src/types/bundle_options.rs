use serde::{
    Deserialize,
    Serialize,
};

use super::runlevel::RunLevel;

/// Store options for Longrun and Oneshot
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct BundleOptions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contents: Vec<String>,
    #[serde(default, skip_serializing_if = "RunLevel::is_default")]
    pub runlevel: RunLevel,
}
