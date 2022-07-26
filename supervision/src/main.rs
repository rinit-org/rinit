#![feature(async_closure)]

pub mod exec_script;
pub mod kill_process;
pub mod log_stdio;
pub mod pidfd_send_signal;
pub mod run_short_lived_script;
pub mod signal_wait;
pub mod supervise_long_lived_process;
pub mod supervise_short_lived_process;

use std::path::PathBuf;

use anyhow::Result;
pub use exec_script::exec_script;
use flexi_logger::{
    writers::FileLogWriter,
    Cleanup,
    Criterion,
    FileSpec,
    Naming,
    WriteMode,
};
pub use kill_process::kill_process;
use lexopt::{
    prelude::Long,
    Arg::Value,
};
pub use log_stdio::{
    log_output,
    StdioType,
};
pub use pidfd_send_signal::pidfd_send_signal;
use rinit_service::types::Service;
pub use run_short_lived_script::run_short_lived_script;
pub use signal_wait::signal_wait;
pub use supervise_long_lived_process::supervise_long_lived_process;
pub use supervise_short_lived_process::supervise_short_lived_process;
use tracing::{
    error,
    level_filters::LevelFilter,
};
use tracing_subscriber::FmtSubscriber;

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
    let (file_writer, _fw_handle) = FileLogWriter::builder(
        FileSpec::default()
            .directory(args.logdir.join(service.name()))
            .basename(service.name()),
    )
    .rotate(
        Criterion::Size(1024 * 512),
        Naming::Numbers,
        Cleanup::KeepCompressedFiles(5),
    )
    .append()
    .write_mode(WriteMode::Async)
    .try_build_with_handle()
    .unwrap();

    let subscriber_builder = FmtSubscriber::builder()
        .with_level(false)
        .with_target(false)
        .with_writer(move || file_writer.clone())
        .with_max_level(LevelFilter::INFO);

    // Get ready to trace
    tracing::subscriber::set_global_default(subscriber_builder.finish())
        .expect("setting default subscriber failed");

    let res = match args.service_type {
        ServiceType::Longrun => supervise_long_lived_process(service).await,
        ServiceType::Oneshot(phase) => supervise_short_lived_process(service, &phase).await,
    };
    if let Err(err) = res {
        error!("ERROR ksupervisor {err}")
    }

    Ok(())
}
