use std::{
    env,
    fs,
    io,
    path::{
        Path,
        PathBuf,
    },
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

static DEFAULT_PATH: &str = "/sbin:/bin:/usr/sbin:/usr/bin";
static DEFAULT_CONFIG_PLACEHOLDER: &str = "%%default_config%%";

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub path: Option<PathBuf>,
    pub configdir: Option<PathBuf>,
    pub rundir: Option<PathBuf>,
    pub datadir: Option<PathBuf>,
    pub service_directories: Vec<PathBuf>,
    pub profile_name: Option<String>,
    #[serde(skip)]
    pub system: bool,
}

#[derive(Debug, Snafu)]
pub enum ConfigError {
    #[snafu(display("unable to initialize XDG BaseDirectories"))]
    BaseDirectoriesError { source: xdg::BaseDirectoriesError },
    #[snafu(display("unable to find configuration directory {:?}", configdir))]
    ConfigDirNotFound { configdir: PathBuf },
    #[snafu(display("unable to find configuration file {:?}", config_file))]
    ConfigFileNotFound { config_file: PathBuf },
    #[snafu(display("unable to read configuration file {:?}", config_path))]
    ConfigReadError {
        config_path: PathBuf,
        source: io::Error,
    },
    #[snafu(display("unable to parse configuration file {:?}", config_path))]
    ConfigFormatError {
        config_path: PathBuf,
        source: toml::de::Error,
    },
    #[snafu(display("unable to convert {:?} to string", path))]
    StringConversionError { path: PathBuf },
}

type Result<T, E = ConfigError> = std::result::Result<T, E>;

impl Config {
    pub fn merge(
        &mut self,
        config: Config,
    ) {
        if config.path.is_some() {
            self.path = config.path;
        }
        if config.rundir.is_some() {
            self.rundir = config.rundir;
        }
        if config.datadir.is_some() {
            self.datadir = config.datadir;
        }
        if !config.service_directories.is_empty() {
            self.service_directories = config.service_directories;
        }
        if config.profile_name.is_some() {
            self.profile_name = config.profile_name;
        }
    }

    pub fn new(opts_configdir: Option<String>) -> Result<Self> {
        let uid = unsafe { libc::getuid() };
        let xdg: BaseDirectories =
            BaseDirectories::with_prefix("kansei").context(BaseDirectoriesSnafu {})?;

        let mut config = if uid == 0 {
            Self::new_default_config()
        } else {
            Self::new_user_config(&xdg)?
        };

        // Merge the config read from the default locations
        let configdir = if let Some(configdir) = &opts_configdir {
            configdir.to_owned()
        } else if uid == 0 {
            "/etc/kansei".to_string()
        } else {
            xdg.get_config_home()
                .to_str()
                .ok_or(ConfigError::StringConversionError {
                    path: xdg.get_config_home(),
                })?
                .to_string()
        };
        let configdir = Path::new(&configdir);
        ensure!(
            opts_configdir.is_none() || configdir.exists(),
            ConfigDirNotFoundSnafu { configdir }
        );
        let config_path = configdir.join("kansei.conf");
        ensure!(
            opts_configdir.is_none() || config_path.exists(),
            ConfigFileNotFoundSnafu {
                config_file: config_path
            }
        );
        if config_path.exists() {
            let config_from_file =
                toml::from_str(&fs::read_to_string(&config_path).with_context(|_| {
                    ConfigReadSnafu {
                        config_path: config_path.clone(),
                    }
                })?)
                .with_context(|_| {
                    ConfigFormatSnafu {
                        config_path: config_path.clone(),
                    }
                })?;
            config.merge(config_from_file);
        }

        // replace configdir placeholder with actual config dir
        // the user might avoid hard writing the configdir
        let configdir = &config.configdir.as_ref().unwrap();
        let new_arr = config
            .service_directories
            .into_iter()
            .map(|dir| -> Result<PathBuf> {
                Ok(if dir.as_os_str() == DEFAULT_CONFIG_PLACEHOLDER {
                    configdir.join("service")
                } else {
                    dir
                })
            })
            .collect::<Result<_>>()?;

        config.service_directories = new_arr;

        Ok(config)
    }

    fn new_default_config() -> Self {
        Config {
            path: Some(PathBuf::from(
                env::var("PATH").unwrap_or_else(|_| DEFAULT_PATH.to_string()),
            )),
            configdir: Some(PathBuf::from("/etc/kansei")),
            rundir: Some(PathBuf::from("/run/kansei")),
            datadir: Some(PathBuf::from("/var/lib/kansei")),
            service_directories: vec![
                PathBuf::from("/etc/kansei/service"),
                PathBuf::from("/usr/share/kansei/service"),
            ],
            profile_name: None,
            system: true,
        }
    }

    fn new_user_config(xdg: &BaseDirectories) -> Result<Self> {
        Ok(Config {
            path: Some(PathBuf::from(
                env::var("PATH").unwrap_or_else(|_| DEFAULT_PATH.to_string()),
            )),
            configdir: { Some(xdg.get_config_home()) },
            rundir: Some(
                xdg.get_runtime_directory()
                    .context(BaseDirectoriesSnafu {})?
                    .join("kansei"),
            ),
            datadir: { Some(xdg.get_data_home()) },
            service_directories: vec![
                PathBuf::from(DEFAULT_CONFIG_PLACEHOLDER),
                PathBuf::from("/usr/share/kansei/service"),
            ],
            profile_name: None,
            system: false,
        })
    }

    pub fn get_graph_filename(&self) -> PathBuf {
        self.datadir.as_ref().unwrap().join("graph.data")
    }
}
