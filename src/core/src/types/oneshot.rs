use serde::{
    Deserialize,
    Serialize,
};

use super::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct Oneshot {
    pub start: Script,
    pub stop: Option<Script>,
    pub options: ServiceOptions,
    pub environment: Config,
}
