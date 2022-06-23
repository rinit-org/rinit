use serde::{
    Deserialize,
    Serialize,
};
use serde_with::skip_serializing_none;

use super::*;

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Oneshot {
    pub name: String,
    pub start: Script,
    pub stop: Option<Script>,
    #[serde(flatten)]
    pub options: ServiceOptions,
}
