use serde::{Deserialize, Serialize};

use super::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct Longrun {
    pub run: Script,
    pub finish: Option<Script>,
    pub options: ServiceOptions,
    pub environment: Config,
}
