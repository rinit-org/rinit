use std::{
    env,
    fs,
    io,
    path::PathBuf,
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

const DIRS_FILE_NAME: &'static str = "dirs.conf";

#[derive(Serialize, Deserialize, Default)]
pub struct CustomDirs {
    pub path: Option<PathBuf>,
    pub configdir: Option<PathBuf>,
    pub rundir: Option<PathBuf>,
    pub datadir: Option<PathBuf>,
    pub logdir: Option<PathBuf>,
    pub profile_name: Option<String>,
}

#[derive(Default, Deserialize)]
pub struct Dirs {
    pub path: PathBuf,
    pub configdir: PathBuf,
    pub rundir: PathBuf,
    pub datadir: PathBuf,
    pub logdir: PathBuf,
}

#[derive(Debug, Snafu)]
pub enum DirsError {
    #[snafu(display("unable to initialize XDG BaseDirectories"))]
    BaseDirectoriesError { source: xdg::BaseDirectoriesError },
    #[snafu(display("unable to find configuration file {:?}", config_file))]
    DirsFileNotFound { config_file: PathBuf },
    #[snafu(display("unable to read configuration file {:?}", config_path))]
    DirsReadError {
        config_path: PathBuf,
        source: io::Error,
    },
    #[snafu(display("unable to parse configuration file {:?}", config_path))]
    DirsFormatError {
        config_path: PathBuf,
        source: toml::de::Error,
    },
    #[snafu(display("unable to convert {:?} to string", path))]
    StringConversionError { path: PathBuf },
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
        // This could also be created as a static, using lazy_static, but local variable
        // is preferred
        let xdg: BaseDirectories =
            BaseDirectories::with_prefix("rinit").context(BaseDirectoriesSnafu {})?;

        // Create a new default directories configuration
        let mut dirs = if uid == 0 {
            Self::new_system_dirs()
        } else {
            Self::new_user_dirs(&xdg)?
        };

        // Calculate the path where the custom config should be
        let custom_dirs_path = dirs.configdir.join(DIRS_FILE_NAME);

        // If there is a custom config
        if custom_dirs_path.exists() {
            // Read and
            let custom_dirs =
                toml::from_str(&fs::read_to_string(&custom_dirs_path).with_context(|_| {
                    DirsReadSnafu {
                        config_path: custom_dirs_path.clone(),
                    }
                })?)
                .with_context(|_| {
                    DirsFormatSnafu {
                        config_path: custom_dirs_path,
                    }
                })?;
            dirs.apply_custom_config(custom_dirs);
        }

        // Read the configuration passed in the command line
        if let Some(opts_dirs_path) = opts_dirs {
            ensure!(
                opts_dirs_path.exists(),
                DirsFileNotFoundSnafu {
                    config_file: opts_dirs_path
                }
            );
            let opts_dirs =
                toml::from_str(&fs::read_to_string(&opts_dirs_path).with_context(|_| {
                    DirsReadSnafu {
                        config_path: opts_dirs_path.clone(),
                    }
                })?)
                .with_context(|_| {
                    DirsFormatSnafu {
                        config_path: opts_dirs_path,
                    }
                })?;
            dirs.apply_custom_config(opts_dirs);
        }

        Ok(dirs)
    }

    fn new_system_dirs() -> Self {
        const DEFAULT_DIRS: &'static str = include_str!("../../dirs.conf");
        toml::from_str(DEFAULT_DIRS).unwrap()
    }

    fn new_user_dirs(xdg: &BaseDirectories) -> Result<Self> {
        let system_config = Dirs::new_system_dirs();
        Ok(Dirs {
            path: env::var("PATH")
                .map(|path| PathBuf::from(path))
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

    pub fn apply_custom_config(
        &mut self,
        config: CustomDirs,
    ) {
        if let Some(path) = config.path {
            self.path = path;
        }
        if let Some(rundir) = config.rundir {
            self.rundir = rundir;
        }
        if let Some(datadir) = config.datadir {
            self.datadir = datadir;
        }
        if let Some(logdir) = config.logdir {
            self.logdir = logdir;
        }
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
