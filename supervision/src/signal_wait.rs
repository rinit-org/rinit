use std::{
    future::Future,
    pin::Pin,
};

use tokio::{
    select,
    signal::unix::{
        signal,
        Signal,
        SignalKind,
    },
    sync::Mutex,
    task::JoinError,
};

lazy_static! {
    static ref SIGINT: Mutex<Signal> = Mutex::new(signal(SignalKind::interrupt()).unwrap());
    static ref SIGTERM: Mutex<Signal> = Mutex::new(signal(SignalKind::terminate()).unwrap());
}
type WaitFn = Pin<Box<dyn Future<Output = Result<(), JoinError>> + Unpin>>;

pub fn signal_wait_fun() -> Box<dyn FnMut() -> WaitFn> {
    Box::new(|| Box::pin(tokio::spawn(async { signal_wait().await })))
}

pub async fn signal_wait() {
    let mut sigint = SIGINT.lock().await;
    let mut sigterm = SIGTERM.lock().await;
    select! {
        _ = sigint.recv() => {},
        _ = sigterm.recv() => {},
    };
}
