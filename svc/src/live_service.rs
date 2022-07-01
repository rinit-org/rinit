use std::{
    cell::RefCell,
    time::Duration,
};

use async_pidfd::PidFd;
use futures::future::BoxFuture;
use rinit_service::{
    graph::Node,
    service_state::ServiceState,
    types::Service,
};
use tokio::{
    io::unix::AsyncFd,
    sync::broadcast::{
        self,
        Receiver,
        Sender,
    },
    time::timeout,
};

// This data will be changed frequently
// To avoid passing &mut LiveService, it is encapsulated by RefCell
pub struct LiveService {
    pub node: Node,
    pub tx: Sender<ServiceState>,
    // Keep a receiving end open so that the sender can always send data
    _rx: Receiver<ServiceState>,
    pub state: RefCell<ServiceState>,
    pub pidfd: RefCell<Option<AsyncFd<PidFd>>>,
    pub remove: bool,
    pub new: Option<Box<LiveService>>,
}

impl LiveService {
    pub fn new(node: Node) -> Self {
        let (tx, rx) = broadcast::channel(1);
        Self {
            node,
            state: RefCell::new(ServiceState::Reset),
            pidfd: RefCell::new(None),
            remove: false,
            new: None,
            tx,
            _rx: rx,
        }
    }

    pub fn get_timeout(&self) -> Duration {
        Duration::from_millis(match &self.node.service {
            Service::Bundle(_) => unreachable!(),
            Service::Longrun(longrun) => {
                if *self.state.borrow() == ServiceState::Starting {
                    longrun.run.timeout * longrun.run.max_deaths as u32
                } else {
                    longrun.run.timeout_kill
                        + if let Some(finish) = &longrun.finish {
                            finish.get_maximum_time()
                        } else {
                            0
                        }
                }
            }
            Service::Oneshot(oneshot) => {
                if *self.state.borrow() == ServiceState::Starting {
                    oneshot.start.get_maximum_time()
                } else if let Some(stop) = &oneshot.stop {
                    stop.get_maximum_time()
                } else {
                    0
                }
            }
            Service::Virtual(_) => unreachable!(),
        } as u64)
    }

    /// Wait until we have one of the 3 final states
    pub fn get_final_state(&self) -> BoxFuture<'static, ServiceState> {
        let state = *self.state.borrow();
        if matches!(state, ServiceState::Starting | ServiceState::Stopping) {
            let mut rx = self.tx.subscribe();
            let service_timeout = self.get_timeout();
            return Box::pin(async move {
                match timeout(service_timeout, rx.recv()).await {
                    Ok(res) => {
                        match res {
                            Ok(state) => state,
                            Err(_) => ServiceState::Down,
                        }
                    }
                    // the wait timed out
                    Err(_) => ServiceState::Down,
                }
            });
        }

        let state = *self.state.borrow();
        Box::pin(async move { state })
    }

    pub fn update_state(
        &self,
        new: ServiceState,
    ) {
        self.state.replace(new);
    }
}
