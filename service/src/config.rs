use std::{
    env,
    path::{
        Path,
        PathBuf,
    },
};

use figment::{
    providers::{
        self,
        Format,
    },
    Figment,
};
use serde::{
    Deserialize,
    Serialize,
};
use snafu::{
    ensure,
    ResultExt,
    Snafu,
};

use crate::dirs::{
    Dirs,
    DirsError,
};

const CONF_FILENAME: &str = "rinit.conf";

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Config {
    pub dirs: Dirs,
}

#[derive(Debug, Snafu)]
pub enum ConfigError {
    #[snafu(display("unable to initialize XDG BaseDirectories"))]
    DirectoriesError { source: DirsError },
    #[snafu(display("unable to find configuration file {:?}", config_file))]
    DirsFileNotFound { config_file: PathBuf },
}

type Result<T, E = ConfigError> = std::result::Result<T, E>;

impl Config {
    pub fn new(opts_conf: Option<PathBuf>) -> Result<Self> {
        let mut conf = Figment::new();

        // This is the configuration read at compile time
        // It is up to packagers to modify accordingly
        conf = conf.merge(providers::Toml::string(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../dirs.conf"
        ))));

        let uid = unsafe { libc::getuid() };

        // Read the conf passed as opt
        conf = if let Some(config_file) = opts_conf {
            ensure!(config_file.exists(), DirsFileNotFoundSnafu { config_file });
            conf.merge(providers::Toml::file(config_file))
        } else if Path::new(CONF_FILENAME).exists() {
            // Read the conf in the current working directory
            conf.merge(providers::Toml::file(Path::new(CONF_FILENAME)))
        } else {
            if uid != 0 {
                conf.merge(providers::Toml::string(
                    &toml::to_string(&Dirs::new_user_dirs().context(DirectoriesSnafu {})?).unwrap(),
                ))
                // Configuration from /etc/rinit/rinit.conf is not read in user
                // mode because the values are completely
                // different.
            } else {
                // read the system configuration
                conf.merge(providers::Toml::file(
                    Dirs::new_system_dirs().configdir.join(CONF_FILENAME),
                ))
            }
        };

        // Read the configuration variables from the env
        conf = conf.merge(providers::Env::prefixed("RINIT_"));

        Ok(conf.extract().unwrap())
    }
}
