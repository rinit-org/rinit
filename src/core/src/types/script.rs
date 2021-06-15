use std::convert::TryFrom;

use libc::SIGINT;
use serde::{
    Deserialize,
    Serialize,
};
use snafu::Snafu;

use super::script_config::ScriptConfig;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum ScriptPrefix {
    Bash,
    Path,
    Sh,
}

#[derive(Snafu, Debug)]
pub struct InvalidScriptPrefixError {
    prefix: String,
}

impl TryFrom<String> for ScriptPrefix {
    type Error = InvalidScriptPrefixError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(match value.as_str() {
            "bash" => ScriptPrefix::Bash,
            "path" => ScriptPrefix::Path,
            "sh" => ScriptPrefix::Sh,
            _ => {
                InvalidScriptPrefixContext {
                    prefix: value.to_owned(),
                }
                .fail()?
            }
        })
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Script {
    pub prefix: ScriptPrefix,
    pub execute: String,
    pub config: ScriptConfig,
    pub timeout: u32,
    pub timeout_kill: u32,
    pub max_deaths: u8,
    pub down_signal: i32,
    pub autostart: bool,
    pub user: Option<String>,
    pub group: Option<String>,
    pub notify: Option<u8>,
}

impl Script {
    pub const DEFAULT_SCRIPT_TIMEOUT: u32 = 3000;
    pub const DEFAULT_SCRIPT_TIMEOUT_KILL: u32 = 3000;
    pub const DEFAULT_SCRIPT_MAX_DEATHS: u8 = 3;

    pub fn new(
        prefix: ScriptPrefix,
        execute: String,
    ) -> Script {
        Script {
            prefix,
            execute,
            config: ScriptConfig::new(),
            timeout: Self::DEFAULT_SCRIPT_TIMEOUT,
            timeout_kill: Self::DEFAULT_SCRIPT_TIMEOUT_KILL,
            max_deaths: Self::DEFAULT_SCRIPT_MAX_DEATHS,
            down_signal: SIGINT,
            autostart: true,
            user: None,
            group: None,
            notify: None,
        }
    }
}
