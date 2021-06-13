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

struct ServiceParser {
    name: String,
}

enum ServiceType {
    Bundle,
    Longrun,
    Oneshot,
    Virtual,
}

#[derive(Snafu, Debug)]
pub enum ParseServiceError {
    #[snafu(display("unable to open file {:?}: {}", path, source))]
    OpenFile { path: PathBuf, source: io::Error },
    #[snafu(display("unable to read line from file {:?}: {}", path, source))]
    ReadFile { path: PathBuf, source: io::Error },
    #[snafu(display("unable to read the name of service in file {:?} at line 1", path))]
    NameNotFound { path: PathBuf },
    #[snafu(display("unable to create service from file {:?}: {}", path, source))]
    ServiceParse {
        path: PathBuf,
        source: ServiceBuilderError,
    },
    #[snafu(display("unable to read service in file {:?}: {}", path, source))]
    ServiceBuild {
        path: PathBuf,
        source: Box<dyn Error>,
    },
    #[snafu(display("unable to read type of service in file {:?} at line 2", path))]
    TypeNotFound { path: PathBuf },
}

type Result<T, E = ParseServiceError> = std::result::Result<T, E>;

macro_rules! read_key_value {
    ($key:literal, $value:tt, $error_type:tt, $reader:tt, $line:tt, $path:tt) => {
        $reader
            .read_line(&mut $line)
            .with_context(|| {
                ReadFile {
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
        .with_context(|| {
            OpenFile {
                path: path.to_owned(),
            }
        })
        .await?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();

    read_key_value!("name", name, NameNotFound, reader, line, path);
    // Otherwise we can't borrow line as mutable again
    let name = name.to_owned();
    line.clear();

    read_key_value!("type", service_type, TypeNotFound, reader, line, path);
    let mut builder = match service_type {
        "bundle" => BundleBuilder::new(name),
        // "longrun" => LongrunParser::parse(name, reader),
        // "oneshot" => OneshotParser::parse(name, reader),
        // "virtual" => VirtualParser::parse(name, reader),
        _ => {
            TypeNotFound {
                path: path.to_owned(),
            }
            .fail()?
        }
    };
    builder
        .parse(
            &reader
                .lines()
                .map(|line| line.unwrap())
                .collect::<Vec<String>>()
                .await,
        )
        .with_context(|| {
            ServiceParse {
                path: path.to_owned(),
            }
        })?;

    builder.build().with_context(|| {
        ServiceBuild {
            path: path.to_owned(),
        }
    })
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
                &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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
}
