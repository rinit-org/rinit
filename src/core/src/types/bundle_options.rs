use serde::{
    Deserialize,
    Serialize,
};

/// Store options for Longrun and Oneshot
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct BundleOptions {
    pub contents: Vec<String>,
}
