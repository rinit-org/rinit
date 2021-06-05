use serde::{
    Deserialize,
    Serialize,
};

use super::*;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Oneshot {
    pub name: String,
    pub start: Script,
    pub stop: Option<Script>,
    pub options: ServiceOptions,
}
