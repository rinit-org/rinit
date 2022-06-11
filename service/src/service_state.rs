use std::fmt;

use serde::{
    Deserialize,
    Serialize,
};

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub enum ServiceState {
    Reset,
    Up,
    Down,
    Starting,
    Stopping,
}

impl fmt::Display for ServiceState {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ServiceState::Reset => "reset",
                ServiceState::Up => "up",
                ServiceState::Down => "down",
                ServiceState::Starting => "starting",
                ServiceState::Stopping => "stopping",
            }
        )
    }
}

unsafe impl Send for ServiceState {}
unsafe impl Sync for ServiceState {}
