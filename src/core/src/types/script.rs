use libc::SIGINT;
use serde::{
    Deserialize,
    Serialize,
};

use super::script_config::ScriptConfig;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum ScriptPrefix {
    Bash,
    Path,
    Sh,
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
