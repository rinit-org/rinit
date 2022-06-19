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

#[derive(Serialize, Deserialize, Default)]
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

    pub fn new(opts_config: Option<PathBuf>) -> Result<Self> {
        let uid = unsafe { libc::getuid() };
        let xdg: BaseDirectories =
            BaseDirectories::with_prefix("rinit").context(BaseDirectoriesSnafu {})?;

        let mut config = if uid == 0 {
            Self::new_default_config()
        } else {
            Self::new_user_config(&xdg)?
        };

        // Merge the config read from the default locations
        let system_config_path = if uid == 0 {
            Path::new("/etc/rinit").to_path_buf()
        } else {
            xdg.get_config_home()
        }
        .join("rinit.conf");

        if system_config_path.exists() {
            let system_config =
                toml::from_str(&fs::read_to_string(&system_config_path).with_context(|_| {
                    ConfigReadSnafu {
                        config_path: system_config_path.clone(),
                    }
                })?)
                .with_context(|_| {
                    ConfigFormatSnafu {
                        config_path: system_config_path,
                    }
                })?;
            config.merge(system_config);
        }

        if let Some(opts_config_file) = &opts_config {
            ensure!(
                opts_config_file.exists(),
                ConfigFileNotFoundSnafu {
                    config_file: opts_config_file
                }
            );
            let opts_config =
                toml::from_str(&fs::read_to_string(&opts_config_file).with_context(|_| {
                    ConfigReadSnafu {
                        config_path: opts_config_file,
                    }
                })?)
                .with_context(|_| {
                    ConfigFormatSnafu {
                        config_path: opts_config_file,
                    }
                })?;
            config.merge(opts_config);
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
            configdir: Some(PathBuf::from("/etc/rinit")),
            rundir: Some(PathBuf::from("/run/rinit")),
            datadir: Some(PathBuf::from("/var/lib/rinit")),
            service_directories: vec![
                PathBuf::from("/etc/rinit/service"),
                PathBuf::from("/usr/share/rinit/service"),
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
                    .join("rinit"),
            ),
            datadir: { Some(xdg.get_data_home()) },
            service_directories: vec![
                PathBuf::from(DEFAULT_CONFIG_PLACEHOLDER),
                PathBuf::from("/usr/share/rinit/service"),
            ],
            profile_name: None,
            system: false,
        })
    }

    pub fn get_graph_filename(&self) -> PathBuf {
        self.datadir.as_ref().unwrap().join("graph.data")
    }
}
