use libc::SIGINT;
use serde::{
    Deserialize,
    Serialize,
};

const DEFAULT_SCRIPT_TIMEOUT: u32 = 3000;
const DEFAULT_SCRIPT_TIMEOUT_KILL: u32 = 3000;
const DEFAULT_SCRIPT_MAX_DEATHS: u8 = 3;

#[derive(Serialize, Deserialize, Debug)]
pub enum ScriptPrefix {
    Bash,
    Path,
    Sh,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Script {
    pub prefix: ScriptPrefix,
    pub execute: String,
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
    pub fn new(
        prefix: ScriptPrefix,
        execute: String,
    ) -> Script {
        Script {
            prefix,
            execute,
            timeout: DEFAULT_SCRIPT_TIMEOUT,
            timeout_kill: DEFAULT_SCRIPT_TIMEOUT_KILL,
            max_deaths: DEFAULT_SCRIPT_MAX_DEATHS,
            down_signal: SIGINT,
            autostart: true,
            user: None,
            group: None,
            notify: None,
        }
    }
}
