use async_condvar_fair::Condvar;
use async_pidfd::PidFd;
use kansei_core::graph::Node;
use tokio::{
    io::unix::AsyncFd,
    sync::Mutex,
};

#[derive(PartialEq, Debug, Clone)]
pub enum ServiceState {
    Reset,
    Up,
    Down,
    Starting,
    Stopping,
}

pub struct LiveServiceStatus {
    pub pidfd: Option<AsyncFd<PidFd>>,
    pub remove: bool,
    pub new: Option<Box<LiveService>>,
}

pub struct LiveService {
    pub node: Node,
    pub state: Mutex<ServiceState>,
    pub wait: Condvar,
    pub status: Mutex<LiveServiceStatus>,
}

impl LiveService {
    pub fn new(node: Node) -> Self {
        Self {
            node,
            state: Mutex::new(ServiceState::Reset),
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
            self.wait.wait_no_relock(state).await;
            state = self.state.lock().await;
        }
        state.clone()
    }
}
