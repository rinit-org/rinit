use std::{
    future::Future,
    pin::Pin,
};

use nix::sys::signal::Signal;
use tokio::{
    select,
    signal::unix::{
        signal,
        SignalKind,
    },
    sync::Mutex,
    task::JoinError,
};

lazy_static! {
    static ref SIGINT: Mutex<tokio::signal::unix::Signal> =
        Mutex::new(signal(SignalKind::interrupt()).unwrap());
    static ref SIGTERM: Mutex<tokio::signal::unix::Signal> =
        Mutex::new(signal(SignalKind::terminate()).unwrap());
}
pub type WaitFn = Pin<Box<dyn Future<Output = Result<Signal, JoinError>> + Unpin>>;

pub fn signal_wait_fun() -> Box<dyn FnMut() -> WaitFn> {
    Box::new(|| Box::pin(tokio::spawn(async { signal_wait().await })))
}

pub async fn signal_wait() -> Signal {
    let mut sigint = SIGINT.lock().await;
    let mut sigterm = SIGTERM.lock().await;
    select! {
        _ = sigint.recv() => Signal::SIGINT,
        _ = sigterm.recv() => Signal::SIGTERM,
    }
}
