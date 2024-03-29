use serde::{
    Deserialize,
    Serialize,
};

use super::bundle_options::BundleOptions;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Bundle {
    pub name: String,
    pub options: BundleOptions,
}
