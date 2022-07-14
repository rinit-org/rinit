use std::str::FromStr;

use serde::{
    Deserialize,
    Serialize,
};
use snafu::Snafu;

// Define the runlevel for the service. Boot is for all the services that needs
// to be started before the others (Default runlevel). It is more obvious for
// root mode but it also makes sense in user mode, where for example you need
// dbus before all the other services
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Default, Clone, Copy)]
pub enum RunLevel {
    Boot,
    #[default]
    Default,
}

#[derive(Debug, Snafu)]
#[snafu(display(""))]
pub struct RunLevelParseError {
    runlevel: String,
}

impl RunLevel {
    pub fn is_default(&self) -> bool {
        matches!(self, RunLevel::Default)
    }
}

impl FromStr for RunLevel {
    type Err = RunLevelParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "boot" => Ok(RunLevel::Boot),
            "default" => Ok(RunLevel::Default),
            _ => {
                RunLevelParseSnafu {
                    runlevel: s.to_string(),
                }
                .fail()
            }
        }
    }
}

impl ToString for RunLevel {
    fn to_string(&self) -> String {
        match self {
            RunLevel::Boot => "boot",
            RunLevel::Default => "default",
        }
        .to_string()
    }
}
