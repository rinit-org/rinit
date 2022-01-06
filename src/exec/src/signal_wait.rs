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

pub fn signal_wait()
-> Box<dyn FnMut() -> Pin<Box<dyn Future<Output = Result<(), JoinError>> + Unpin>>> {
    Box::new(|| {
        Box::pin(tokio::spawn(async {
            let mut sigint = SIGINT.lock().await;
            let mut sigterm = SIGTERM.lock().await;
            select! {
                _ = sigint.recv() => {},
                _ = sigterm.recv() => {},
            };
        }))
    })
}
