use std::{
    env,
    path::PathBuf,
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
use xdg::BaseDirectories;

const DIRS_FILE_NAME: &str = "dirs.conf";

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Dirs {
    #[serde(default)]
    pub path: PathBuf,
    #[serde(default)]
    pub configdir: PathBuf,
    #[serde(default)]
    pub rundir: PathBuf,
    #[serde(default)]
    pub datadir: PathBuf,
    #[serde(default)]
    pub logdir: PathBuf,
}

#[derive(Debug, Snafu)]
pub enum DirsError {
    #[snafu(display("unable to initialize XDG BaseDirectories"))]
    BaseDirectoriesError { source: xdg::BaseDirectoriesError },
    #[snafu(display("unable to find configuration file {:?}", config_file))]
    DirsFileNotFound { config_file: PathBuf },
}

type Result<T, E = DirsError> = std::result::Result<T, E>;

impl Dirs {
    // opts_config is a path passed in the command line
    // while it is not needed for system mode, since packagers can just change
    // dirs.yml, users with a custom directory configuration must pass the config
    // in this value
    pub fn new(opts_dirs: Option<PathBuf>) -> Result<Self> {
        let uid = unsafe { libc::getuid() };
        // Always initialize an xdg var, we will need it for user values
        // In case this is the system mode, xdg does not create any dir, no harm done
        // Note: This could also be created as a static variable, using lazy_static, but
        // local variable is preferred
        let xdg: BaseDirectories =
            BaseDirectories::with_prefix("rinit").context(BaseDirectoriesSnafu {})?;

        let mut dirs = Figment::new();
        // Get the default configuration depending on the mode
        if uid == 0 {
            dirs = dirs
                .merge(providers::Toml::string(
                    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../dirs.conf")),
                ))
                // Read the system configuration
                .merge(providers::Toml::file(
                    Self::new_system_dirs().configdir.join(DIRS_FILE_NAME),
                ));
        } else {
            dirs = dirs.merge(providers::Toml::string(
                &toml::to_string(&Self::new_user_dirs(&xdg)?).unwrap(),
            ));
            // Configuration from /etc/rinit/dirs.conf is not read in user mode
            // because the values are completely different.
            // TODO: Add a /etc/rinit/user/env with RINIT_ env values
        }

        // Read the configuration passed in the command line
        if let Some(opts_dirs_path) = opts_dirs {
            ensure!(
                opts_dirs_path.exists(),
                DirsFileNotFoundSnafu {
                    config_file: opts_dirs_path
                }
            );
            dirs = dirs.merge(providers::Toml::file(opts_dirs_path));
        }

        // Read the configuration variables from the env
        dirs = dirs.merge(providers::Env::prefixed("RINIT_"));

        Ok(dirs.extract().unwrap())
    }

    /// Get the default directories for the system mode
    fn new_system_dirs() -> Self {
        // The default hardcoded configuration is read from dirs.conf
        toml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../dirs.conf"
        )))
        .unwrap()
    }

    /// Get the default directories for the user mode
    /// These are generated at runtime using xdg directory standard
    fn new_user_dirs(xdg: &BaseDirectories) -> Result<Self> {
        let system_config = Dirs::new_system_dirs();
        Ok(Dirs {
            path: env::var("PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| system_config.path),
            configdir: xdg.get_config_home(),
            rundir: xdg
                .get_runtime_directory()
                .context(BaseDirectoriesSnafu {})?
                .join("rinit"),
            datadir: xdg.get_data_home(),
            logdir: xdg.get_state_home(),
        })
    }

    pub fn service_directories(&self) -> Vec<PathBuf> {
        let uid = unsafe { libc::getuid() };
        let service_type = if uid == 0 { "system" } else { "user" };
        let mut dirs = vec![
            self.configdir.join(service_type),
            self.datadir.join(service_type),
        ];
        if uid == 0 {
            dirs.push(Self::new_system_dirs().datadir.join("user"))
        }
        dirs
    }

    pub fn graph_filename(&self) -> PathBuf {
        self.datadir.join("graph.data")
    }
}
