use std::path::{
    Path,
    PathBuf,
};

use async_std::{
    fs::File,
    io::{
        self,
        BufReader,
    },
    prelude::*,
};
use futures::stream::StreamExt;
use kansei_core::types::*;
use snafu::{
    ensure,
    futures::TryFutureExt,
    Error,
    OptionExt,
    ResultExt,
    Snafu,
};

use crate::service::service_builder::*;

#[derive(Snafu, Debug)]
pub enum ParseServiceError {
    #[snafu(display("unable to open file {:?}", path))]
    OpenFile { path: PathBuf, source: io::Error },
    #[snafu(display("unable to read line from file {:?}", path))]
    ReadFile { path: PathBuf, source: io::Error },
    #[snafu(display("unable to read the name of service in file {:?} at line 1", path))]
    NameNotFound { path: PathBuf },
    #[snafu(display("while reading file {:?}", path))]
    ServiceParse {
        path: PathBuf,
        source: ServiceBuilderError,
    },
    #[snafu(display("whiile reading file {:?}", path))]
    ServiceBuild {
        path: PathBuf,
        source: Box<dyn Error>,
    },
    #[snafu(display("unable to read type of service in file {:?} at line 2", path))]
    TypeNotFound { path: PathBuf },
}

unsafe impl Send for ParseServiceError {}

type Result<T, E = ParseServiceError> = std::result::Result<T, E>;

macro_rules! read_key_value {
    ($key:literal, $value:tt, $error_type:tt, $reader:tt, $line:tt, $path:tt) => {
        $reader
            .read_line(&mut $line)
            .with_context(|_| {
                ReadFileSnafu {
                    path: $path.clone(),
                }
            })
            .await?;
        let (key, $value) = $line.split_once('=').with_context(|| {
            $error_type {
                path: $path.clone(),
            }
        })?;
        let $value = $value.trim();
        ensure!(
            key.trim() == $key,
            $error_type {
                path: $path.clone()
            }
        );
    };
}

pub async fn parse_service(path: &Path) -> Result<Service> {
    let file = File::open(&path)
        .with_context(|_| {
            OpenFileSnafu {
                path: path.to_owned(),
            }
        })
        .await?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();

    read_key_value!("name", name, NameNotFoundSnafu, reader, line, path);
    // Otherwise we can't borrow line as mutable again
    let name = name.to_owned();
    line.clear();

    read_key_value!("type", service_type, TypeNotFoundSnafu, reader, line, path);
    let lines = &reader
        .lines()
        .map(|line| line.unwrap())
        .collect::<Vec<String>>()
        .await;
    match service_type {
        "bundle" => {
            let mut builder = BundleBuilder::new(name);
            builder.parse(lines).with_context(|_| {
                ServiceParseSnafu {
                    path: path.to_owned(),
                }
            })?;

            builder.build().with_context(|_| {
                ServiceBuildSnafu {
                    path: path.to_owned(),
                }
            })
        }
        "longrun" => {
            let mut builder = LongrunBuilder::new(name);
            builder.parse(lines).with_context(|_| {
                ServiceParseSnafu {
                    path: path.to_owned(),
                }
            })?;

            builder.build().with_context(|_| {
                ServiceBuildSnafu {
                    path: path.to_owned(),
                }
            })
        }
        "oneshot" => {
            let mut builder = OneshotBuilder::new(name);
            builder.parse(lines).with_context(|_| {
                ServiceParseSnafu {
                    path: path.to_owned(),
                }
            })?;

            builder.build().with_context(|_| {
                ServiceBuildSnafu {
                    path: path.to_owned(),
                }
            })
        }
        // "virtual" => VirtualParser::parse(name, reader),
        _ => {
            TypeNotFoundSnafu {
                path: path.to_owned(),
            }
            .fail()?
        }
    }
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;

    use async_std::task;

    use super::*;

    #[test]
    fn parse_bundle() -> Result<(), ParseServiceError> {
        assert_eq!(
            Service::Bundle(Bundle {
                name: "foo".to_string(),
                options: BundleOptions {
                    contents: vec!["bar".to_string()]
                }
            }),
            task::block_on(parse_service(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("test/samples/bundle")
                    .as_path()
            ))?
        );

        Ok(())
    }

    #[test]
    fn parse_bundle_no_options() -> Result<(), ParseServiceError> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test/samples/bundle_no_options");
        assert!(task::block_on(parse_service(&path)).is_err());

        Ok(())
    }

    #[test]
    fn parse_oneshot() -> Result<(), ParseServiceError> {
        assert_eq!(
            Service::Oneshot(Oneshot {
                name: "foo".to_string(),
                start: Script::new(ScriptPrefix::Bash, "    exit 0".to_string()),
                stop: None,
                options: ServiceOptions::new(),
            }),
            task::block_on(parse_service(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("test/samples/oneshot")
                    .as_path()
            ))?
        );

        Ok(())
    }

    #[test]
    fn parse_oneshot_with_stop() -> Result<(), ParseServiceError> {
        assert_eq!(
            Service::Oneshot(Oneshot {
                name: "foo".to_string(),
                start: Script::new(ScriptPrefix::Bash, "    exit 0".to_string()),
                stop: Some(Script::new(ScriptPrefix::Sh, "    exit 1".to_string())),
                options: ServiceOptions::new(),
            }),
            task::block_on(parse_service(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("test/samples/oneshot_with_stop")
                    .as_path()
            ))?
        );

        Ok(())
    }

    #[test]
    fn parse_longrun() -> Result<(), ParseServiceError> {
        assert_eq!(
            Service::Longrun(Longrun {
                name: "foo".to_string(),
                run: Script::new(ScriptPrefix::Bash, "    loop".to_string()),
                finish: None,
                options: ServiceOptions::new(),
            }),
            task::block_on(parse_service(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("test/samples/longrun")
                    .as_path()
            ))?
        );

        Ok(())
    }

    #[test]
    fn parse_longrun_no_run() {
        assert!(
            task::block_on(parse_service(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("test/samples/longrun_no_run")
                    .as_path()
            ))
            .is_err()
        );
    }
}
