use std::{
    error::Error,
    path::{
        Path,
        PathBuf,
    },
};

use futures::{
    future::BoxFuture,
    Future,
};
use kansei_core::types::Service;
use snafu::{
    ResultExt,
    Snafu,
};

use crate::{
    parse_service,
    ParseServiceError,
};

#[derive(Snafu, Debug)]
pub enum ServicesParserError {
    #[snafu(display("error while parsing service, {}", source))]
    ParsingServiceError { source: ParseServiceError },
    #[snafu(display("could not find service file for {:?}", service))]
    CouldNotFindService { service: String },
}

unsafe impl Send for ServicesParserError {}
unsafe impl Sync for ServicesParserError {}

pub async fn parse_services(
    services: Vec<String>,
    service_dirs: &Vec<PathBuf>,
    system: bool,
) -> Result<Vec<Service>, ServicesParserError> {
    let mut services_already_parsed = services.clone();
    let mut results = Vec::new();
    let mut currently_parsing: Vec<BoxFuture<'static, Result<Service, ParseServiceError>>> =
        services
            .into_iter()
            .map(|service| {
                if let Some(val) = get_service_file(&service, service_dirs, system) {
                    Ok(val)
                } else {
                    Err(service)
                }
            })
            .collect::<Result<Vec<PathBuf>, String>>()
            .map_err(|service| ServicesParserError::CouldNotFindService { service })?
            .into_iter()
            .map(parse_service_future)
            .collect();

    while let Some(future) = currently_parsing.pop() {
        let service = future.await.context(ParsingServiceError {})?;
        let mut dependencies: Vec<String> = service.dependencies().into();

        results.push(service);
        currently_parsing.extend(
            dependencies
                .iter()
                // Skip services that we can't found, the dependency graph will handle the error
                .filter_map(|service| get_service_file(&service, service_dirs, system))
                .map(parse_service_future),
        );

        services_already_parsed.append(&mut dependencies);
    }

    Ok(results)
}

fn parse_service_future(
    service_file: PathBuf
) -> BoxFuture<'static, Result<Service, ParseServiceError>> {
    Box::pin(async move { parse_service(&service_file).await })
}

fn get_service_file(
    service: &str,
    paths: &Vec<PathBuf>,
    system: bool,
) -> Option<PathBuf> {
    paths.iter().find_map(|path| {
        let service_file =
            path.join(Path::new(service).with_extension(if system { "system" } else { "user" }));
        if service_file.exists() {
            Some(service_file)
        } else {
            None
        }
    })
}
