#![feature(async_closure)]
#![feature(let_chains)]

pub mod exec_script;
pub mod kill_process;
pub mod pidfd_send_signal;
pub mod run_short_lived_script;
pub mod signal_wait;
pub mod supervise_long_lived_process;
pub mod supervise_short_lived_process;

use std::path::PathBuf;

use anyhow::Result;
pub use exec_script::exec_script;
pub use kill_process::kill_process;
use lexopt::{
    prelude::Long,
    Arg::Value,
};
pub use pidfd_send_signal::pidfd_send_signal;
use rinit_service::types::Service;
pub use run_short_lived_script::run_short_lived_script;
pub use signal_wait::signal_wait;
pub use supervise_long_lived_process::supervise_long_lived_process;
pub use supervise_short_lived_process::supervise_short_lived_process;
use tracing::{
    error,
    metadata::LevelFilter,
};

#[macro_use]
extern crate lazy_static;

#[derive(Debug)]
enum ServiceType {
    Longrun,
    Oneshot(String),
}

#[derive(Debug)]
struct Args {
    service_type: ServiceType,
    logdir: PathBuf,
    service: String,
}

fn parse_args() -> Result<Args, lexopt::Error> {
    let mut logdir: Option<PathBuf> = None;
    let mut service_type: Option<ServiceType> = None;
    let mut service: Option<String> = None;
    let mut parser = lexopt::Parser::from_env();
    while let Some(arg) = parser.next()? {
        match arg {
            Long("logdir") => {
                logdir = Some(PathBuf::from(parser.value()?));
            }
            Long("longrun") => {
                service_type = Some(ServiceType::Longrun);
                // This value is not used but it's set so
                // that oneshot and longrun can be spawned with the same syntax
                parser.value()?;
            }
            Long("oneshot") => {
                service_type = Some(ServiceType::Oneshot(
                    parser.value()?.to_string_lossy().to_string(),
                ));
            }
            Long("help") => {
                println!("Usage: rsvc [-c|--config=CONFIG]");
                std::process::exit(0);
            }
            Value(val) if service.is_none() => {
                service = Some(val.into_string()?);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    Ok(Args {
        logdir: logdir.expect("logdir is not set"),
        service_type: service_type.expect("service type is not set"),
        service: service.expect("the service has not been provided"),
    })
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = parse_args()?;
    let service: Service = serde_json::from_str(&args.service)?;

    // Setup logging
    use tracing_error::ErrorLayer;
    use tracing_subscriber::{
        fmt,
        prelude::*,
        EnvFilter,
    };

    let file_appender = tracing_appender::rolling::daily(
        args.logdir.join(service.name()),
        format!("{}.log", service.name()),
    );
    let (service_log_writer, _guard) = tracing_appender::non_blocking(file_appender);
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();
    use tracing_subscriber::fmt::format;
    let stdio_fmt_layer = fmt::layer()
        .event_format(format())
        .with_target(false)
        // Do not write warn and info to stdout, as it's inherited from rsvc
        .with_filter(LevelFilter::ERROR);
    let file_fmt_layer = fmt::layer()
        .with_target(false)
        .with_level(false)
        .with_writer(service_log_writer);

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(stdio_fmt_layer)
        .with(file_fmt_layer)
        .with(ErrorLayer::default())
        .init();

    let res = match args.service_type {
        ServiceType::Longrun => supervise_long_lived_process(service).await,
        ServiceType::Oneshot(phase) => supervise_short_lived_process(service, &phase).await,
    };
    if let Err(err) = res {
        error!("ERROR ksupervisor {err}")
    }

    Ok(())
}
