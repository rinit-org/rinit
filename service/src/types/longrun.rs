use serde::{
    Deserialize,
    Serialize,
};
use serde_with::skip_serializing_none;

use super::*;

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct Longrun {
    pub name: String,
    pub run: Script,
    pub finish: Option<Script>,
    #[serde(flatten)]
    pub options: ServiceOptions,
    #[serde(flatten, default, skip_serializing_if = "ScriptEnvironment::is_empty")]
    pub environment: ScriptEnvironment,
}
