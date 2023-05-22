use std::{
    env,
    path::PathBuf,
};

use serde::{
    Deserialize,
    Serialize,
};
use snafu::{
    ResultExt,
    Snafu,
};
use xdg::BaseDirectories;

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
}

impl Dirs {
    /// Get the default directories for the system mode
    pub fn new_system_dirs() -> Self {
        // The default hardcoded configuration is read from dirs.conf
        toml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../dirs.conf"
        )))
        .unwrap()
    }

    /// Get the default directories for the user mode
    /// These are generated at runtime using xdg directory standard
    pub fn new_user_dirs() -> Result<Self, DirsError> {
        let xdg: BaseDirectories =
            BaseDirectories::with_prefix("rinit").context(BaseDirectoriesSnafu {})?;
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
