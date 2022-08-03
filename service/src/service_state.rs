use std::fmt;

use serde::{
    Deserialize,
    Serialize,
};

// Put service state in crate service because it's used by ipc and svc crates
/// Represent two different states
#[derive(PartialEq, Eq, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ServiceState {
    /// The service has reached an idle state and will not change without
    /// changes from outside, e.g. it fails or the user asks for a change
    Idle(IdleServiceState),
    /// The service is a transitioning and temporary status. We are currently
    /// waiting for supervisor to give us an update
    Transitioning(TransitioningServiceState),
}

#[derive(PartialEq, Eq, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum IdleServiceState {
    Up,
    Down,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TransitioningServiceState {
    Starting,
    Stopping,
}

impl fmt::Display for ServiceState {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            ServiceState::Idle(state) => write!(f, "{state}"),
            ServiceState::Transitioning(state) => write!(f, "{state}"),
        }
    }
}

impl fmt::Display for IdleServiceState {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                IdleServiceState::Up => "up",
                IdleServiceState::Down => "down",
            }
        )
    }
}

impl fmt::Display for TransitioningServiceState {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                TransitioningServiceState::Starting => "starting",
                TransitioningServiceState::Stopping => "stopping",
            }
        )
    }
}

unsafe impl Send for ServiceState {}
unsafe impl Sync for ServiceState {}
