use anyhow::Result;
use rinit_ipc::Message;
use rinit_service::types::Oneshot;

use crate::{
    run_short_lived_script,
    signal_wait::signal_wait_fun,
};

pub async fn supervise_short_lived_process(
    phase: &str,
    service: &str,
) -> Result<()> {
    let start = match phase {
        "start" => true,
        "stop" => false,
        _ => todo!(),
    };
    let oneshot: Oneshot = serde_json::from_str(service)?;
    let message = Message::ServiceIsUp(
        if start {
            run_short_lived_script(&oneshot.start, signal_wait_fun()).await?
        } else {
            if let Some(stop_script) = oneshot.stop {
                run_short_lived_script(&stop_script, signal_wait_fun()).await?;
            }
            false
        },
        oneshot.name,
    );

    // TODO: log this
    message.send().unwrap();

    Ok(())
}
