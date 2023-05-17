use anyhow::Result;
use rinit_ipc::{
    AsyncConnection,
    Reply,
    Request,
};
use rinit_service::types::RunLevel;

pub async fn start_service(
    conn: &mut AsyncConnection,
    service: &str,
    runlevel: RunLevel,
) -> Result<bool> {
    let request = Request::StartService {
        service: service.to_owned(),
        runlevel,
    };
    match conn.send_request(request).await?? {
        Reply::Success(success) => Ok(success),
        _ => unreachable!(),
    }
}
