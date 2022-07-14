use std::str::FromStr;

use serde::{
    Deserialize,
    Serialize,
};
use snafu::NoneError;

// Define the runlevel for the service. Boot is for all the services that needs
// to be started before the others (Default runlevel). It is more obvious for
// root mode but it also makes sense in user mode, where for example you need
// dbus before all the other services
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Default, Clone)]
pub enum RunLevel {
    Boot,
    #[default]
    Default,
}

impl RunLevel {
    pub fn is_default(&self) -> bool {
        matches!(self, RunLevel::Default)
    }
}

impl FromStr for RunLevel {
    type Err = NoneError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "boot" => RunLevel::Boot,
            "default" => RunLevel::Default,
            _ => return Err(NoneError {}),
        })
    }
}
