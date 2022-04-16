use std::{
    sync::Arc,
    time::Duration,
};

use async_condvar_fair::Condvar;
use async_pidfd::PidFd;
use kansei_core::{
    graph::Node,
    service_state::ServiceState,
    types::Service,
};
use tokio::{
    io::unix::AsyncFd,
    sync::Mutex,
    time::timeout,
};

pub struct LiveServiceStatus {
    pub pidfd: Option<AsyncFd<PidFd>>,
    pub remove: bool,
    pub new: Option<Box<LiveService>>,
}

pub struct LiveService {
    pub node: Node,
    pub state: Arc<Mutex<ServiceState>>,
    pub wait: Condvar,
    pub status: Mutex<LiveServiceStatus>,
}

impl LiveService {
    pub fn new(node: Node) -> Self {
        Self {
            node,
            state: Arc::new(Mutex::new(ServiceState::Reset)),
            wait: Condvar::new(),
            status: Mutex::new(LiveServiceStatus {
                pidfd: None,
                remove: false,
                new: None,
            }),
        }
    }

    /// Wait until we have one of the 3 final states
    pub async fn get_final_state(&self) -> ServiceState {
        let mut state = self.state.lock().await;
        if match *state {
            ServiceState::Starting | ServiceState::Stopping => true,
            _ => false,
        } {
            let service_timeout = Duration::from_millis(match &self.node.service {
                Service::Bundle(_) => unreachable!(),
                Service::Longrun(longrun) => {
                    if *state == ServiceState::Starting {
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
                    if *state == ServiceState::Starting {
                        oneshot.start.get_maximum_time()
                    } else {
                        if let Some(stop) = &oneshot.stop {
                            stop.get_maximum_time()
                        } else {
                            0
                        }
                    }
                }
                Service::Virtual(_) => unreachable!(),
            } as u64);
            if let Err(_) = timeout(service_timeout, self.wait.wait_no_relock(state)).await {
                // the wait timed out
                return ServiceState::Down;
            }
            state = self.state.lock().await;
        }
        state.clone()
    }
}
