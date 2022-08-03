use std::{
    cell::RefCell,
    time::Duration,
};

use futures::future::BoxFuture;
use rinit_service::{
    graph::Node,
    service_state::{
        IdleServiceState,
        ServiceState,
        TransitioningServiceState,
    },
    types::Service,
};
use tokio::{
    process::Child,
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
    // TransitioningServiceState is only internal and should never be sent/received
    pub tx: Sender<IdleServiceState>,
    // Keep a receiving end open so that the sender can always send data
    _rx: Receiver<IdleServiceState>,
    pub state: RefCell<ServiceState>,
    pub child: RefCell<Option<Child>>,
    pub remove: bool,
    pub new: Option<Box<LiveService>>,
}

impl LiveService {
    pub fn new(node: Node) -> Self {
        let (tx, rx) = broadcast::channel(1);
        Self {
            node,
            state: RefCell::new(ServiceState::Idle(IdleServiceState::Down)),
            child: RefCell::new(None),
            remove: false,
            new: None,
            tx,
            _rx: rx,
        }
    }

    pub fn get_timeout(&self) -> Duration {
        Duration::from_millis(match *self.state.borrow() {
            ServiceState::Idle(_) => unreachable!(),
            ServiceState::Transitioning(state) => {
                match state {
                    TransitioningServiceState::Starting => {
                        match &self.node.service {
                            Service::Bundle(_) => unreachable!(),
                            Service::Longrun(longrun) => {
                                longrun.run.timeout * longrun.run.max_deaths as u32
                            }
                            Service::Oneshot(oneshot) => oneshot.start.get_maximum_time(),
                            Service::Virtual(_) => todo!(),
                        }
                    }
                    TransitioningServiceState::Stopping => {
                        match &self.node.service {
                            Service::Bundle(_) => unreachable!(),
                            Service::Longrun(longrun) => {
                                longrun.run.timeout_kill
                                    + if let Some(finish) = &longrun.finish {
                                        finish.get_maximum_time()
                                    } else {
                                        0
                                    }
                            }
                            Service::Oneshot(oneshot) => {
                                if let Some(stop) = &oneshot.stop {
                                    stop.get_maximum_time()
                                } else {
                                    0
                                }
                            }
                            Service::Virtual(_) => todo!(),
                        }
                    }
                }
            }
        } as u64)
    }

    /// Wait until we have an idle service state, i.e. non transitioning
    /// A BoxFuture is returned so that it's independent from the live_service
    pub fn wait_idle_state(&self) -> BoxFuture<'static, IdleServiceState> {
        let state = *self.state.borrow();
        match state {
            ServiceState::Transitioning(_) => {
                let mut rx = self.tx.subscribe();
                let service_timeout = self.get_timeout();
                Box::pin(async move {
                    match timeout(service_timeout, rx.recv()).await {
                        Ok(res) => {
                            match res {
                                Ok(state) => state,
                                Err(_) => IdleServiceState::Down,
                            }
                        }
                        // the wait timed out
                        Err(_) => IdleServiceState::Down,
                    }
                })
            }
            ServiceState::Idle(state) => Box::pin(async move { state }),
        }
    }

    pub fn update_state(
        &self,
        new: ServiceState,
    ) {
        self.state.replace(new);
    }
}
