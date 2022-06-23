use std::{
    cell::RefCell,
    time::Duration,
};

use async_pidfd::PidFd;
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
    pub tx: Sender<()>,
    // Keep a receiving end open so that the sender can always send data
    _rx: Receiver<()>,
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
                    longrun.run.timeout.unwrap() * longrun.run.max_deaths.unwrap() as u32
                } else {
                    longrun.run.timeout_kill.unwrap()
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
    pub async fn get_final_state(&self) -> ServiceState {
        let state = *self.state.borrow();
        if matches!(state, ServiceState::Starting | ServiceState::Stopping)
            && timeout(self.get_timeout(), self.tx.subscribe().recv())
                .await
                .is_err()
        {
            // the wait timed out
            return ServiceState::Down;
        }
        *self.state.borrow()
    }

    pub fn update_state(
        &self,
        new: ServiceState,
    ) {
        self.state.replace(new);
    }
}
