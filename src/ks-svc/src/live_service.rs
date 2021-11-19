use anyhow::Result;
use async_pidfd::PidFd;
use chrono::prelude::*;
use kansei_core::{
    graph::Node,
    types::ScriptConfig,
};

#[derive(Clone)]
pub enum ServiceStatus {
    Reset,
    Up,
    Down,
    Starting,
    Stopping,
}

pub struct LiveService {
    pub node: Node,
    pub updated_node: Option<Node>,
    pub status: ServiceStatus,
    pub status_changed: Option<DateTime<Local>>,
    // Skip starting and stopping values here
    pub last_status: Option<ServiceStatus>,
    // first element for Oneshot::start and Longrun::run
    // second element for Oneshot::stop and Longrun::finish
    pub config: Option<(ScriptConfig, ScriptConfig)>,
    pub environment: Option<(ScriptConfig, ScriptConfig)>,
    pub remove: bool,
    pub supervisor: Option<PidFd>,
}

impl LiveService {
    pub fn new(node: Node) -> Self {
        Self {
            node,
            updated_node: None,
            status: ServiceStatus::Reset,
            status_changed: None,
            last_status: None,
            config: None,
            environment: None,
            remove: false,
            supervisor: None,
        }
    }

    pub fn change_status(
        &mut self,
        new_status: ServiceStatus,
    ) {
        match self.status {
            ServiceStatus::Starting => {}
            ServiceStatus::Stopping => {}
            _ => {
                self.last_status = Some(self.status.clone());
            }
        }
        self.status = new_status;
        self.status_changed = Some(chrono::offset::Local::now());
    }
}
