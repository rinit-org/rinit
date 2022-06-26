use std::io;

use remoc::{
    chmux::ChMuxError,
    rch,
};
use snafu::{
    OptionExt,
    ResultExt,
    Snafu,
};
use tokio::{
    net::UnixStream,
    task,
};

use crate::{
    request_error::RequestError,
    Reply,
    Request,
};

pub struct AsyncConnection {
    tx: rch::base::Sender<Request>,
    rx: rch::base::Receiver<Result<Reply, RequestError>>,
}

#[derive(Snafu, Debug)]
pub enum ConnectionError<T>
where
    T: std::fmt::Debug + 'static,
{
    #[snafu(display("error while connecting to socket {socket}: {source}"))]
    SocketConnectionError { socket: String, source: io::Error },
    #[snafu(display("error while connecting : {source}"), context(false))]
    ConnectError {
        source: remoc::ConnectError<io::Error, io::Error>,
    },
    #[snafu(
        display("error while establishing a chmux connection: {source}"),
        context(false)
    )]
    ConnectChMuxError {
        source: ChMuxError<io::Error, io::Error>,
    },
    #[snafu(display("error while receiving a request: {source}"), context(false))]
    ReceiveError { source: rch::base::RecvError },
    #[snafu(display("error while sending a reply: {source}"), context(false))]
    SendError { source: rch::base::SendError<T> },
    #[snafu(display("no reply received for request {request:?}"))]
    NoReplyReceived { request: Request },
}

// Ideally there should be async and sync connection, but
// remoc is async and requires to spawn a new task. This functions cannot
// be sync, as Runtime::blowk_on would suspend any non-finished tasks upon
// completion, including the one that remoc requires to spawn
// remoc is an awesome crate for handling the connection and it is worth
// to have all the application as (single-threaded) async.
// Keep in mind that any external communication will be handled by another
// daemon and not by svc/ctl itself, so that remoc is not enforced nor
// required for interacting with rinit
impl AsyncConnection {
    pub async fn new(socket: &str) -> Result<Self, ConnectionError<Request>> {
        let stream = UnixStream::connect(socket).await.with_context(|_| {
            SocketConnectionSnafu {
                socket: socket.to_string(),
            }
        })?;
        let (socket_rx, socket_tx) = stream.into_split();
        let (conn, tx, rx): (
            _,
            rch::base::Sender<Request>,
            rch::base::Receiver<Result<Reply, RequestError>>,
        ) = remoc::Connect::io(remoc::Cfg::default(), socket_rx, socket_tx).await?;
        task::spawn(conn);

        Ok(Self { tx, rx })
    }

    pub async fn new_host_address() -> Result<Self, ConnectionError<Request>> {
        Self::new(crate::get_host_address()).await
    }

    pub async fn send_request(
        &mut self,
        request: Request,
    ) -> Result<Result<Reply, RequestError>, ConnectionError<Request>> {
        self.tx.send(request.clone()).await?;
        self.rx
            .recv()
            .await?
            .with_context(|| NoReplyReceivedSnafu { request })
    }
}
