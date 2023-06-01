use std::{
    cell::RefCell,
    path::Path,
    time::Duration,
};

use flexi_logger::{
    writers::{
        FileLogWriter,
        FileLogWriterHandle,
    },
    Cleanup,
    Criterion,
    FileSpec,
    Naming,
    WriteMode,
};
use futures::future::BoxFuture;
use rinit_ipc::Request;
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
    sync::{
        broadcast,
        mpsc,
        watch,
    },
    task,
    time::timeout,
};
use tracing::{
    error,
    instrument::WithSubscriber,
    metadata::LevelFilter,
    warn,
};
use tracing_subscriber::FmtSubscriber;

use crate::supervision::{
    run_short_lived_script,
    Supervisor,
};

// This data will be changed frequently
// To avoid passing &mut LiveService, it is encapsulated by RefCell
pub struct LiveService {
    pub node: Node,
    // TransitioningServiceState is only internal and should never be sent/received
    pub tx: broadcast::Sender<IdleServiceState>,
    // Keep a receiving end open so that the sender can always send data
    _rx: broadcast::Receiver<IdleServiceState>,
    pub state: RefCell<ServiceState>,
    pub terminate: RefCell<Option<watch::Sender<()>>>,
    pub remove: bool,
    pub new: Option<Box<LiveService>>,
}

impl LiveService {
    pub fn new(node: Node) -> Self {
        let (tx, rx) = broadcast::channel(1);
        Self {
            node,
            state: RefCell::new(ServiceState::Idle(IdleServiceState::Down)),
            remove: false,
            new: None,
            tx,
            _rx: rx,
            terminate: RefCell::new(None),
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

    pub async fn start_service(
        &self,
        logdir: &Path,
        send: mpsc::Sender<Request>,
    ) -> bool {
        match &self.node.service {
            Service::Longrun(longrun) => {
                let (tx, rx) = watch::channel(());
                // terminate is our channel to ask the supervisor to close the process
                self.terminate.replace(Some(tx));
                let (fw_handle, logger) = self.logger_subscriber(logdir);
                let mut supervisor = Supervisor::new(longrun.clone(), rx, fw_handle);
                async {
                    match supervisor.start().await {
                        Ok(res) => {
                            if res {
                                task::spawn_local(async move {
                                    // We need to pass send because it will be used to notify
                                    if let Err(err) = supervisor.supervise(send).await {
                                        error!("{err}");
                                    }
                                });
                            }
                            res
                        }
                        Err(err) => {
                            error!("{err}");
                            false
                        }
                    }
                }
                .with_subscriber(logger)
                .await
            }
            Service::Oneshot(oneshot) => {
                run_short_lived_script(&oneshot.start, &oneshot.environment)
                    .with_subscriber(self.logger_subscriber(logdir).1)
                    .await
                    .unwrap()
            }
            Service::Bundle(_) | Service::Virtual(_) => todo!(),
        }
    }

    pub async fn stop_service(
        &self,
        logdir: &Path,
    ) {
        match &self.node.service {
            Service::Longrun(_) => {
                if let Some(terminate) = &*self.terminate.borrow() {
                    // Ask the supervisor to close the process
                    if let Err(err) = terminate.send(()) {
                        warn!("{err}");
                    }
                }
            }
            Service::Oneshot(oneshot) => {
                if let Some(stop_script) = &oneshot.stop {
                    let res = run_short_lived_script(stop_script, &oneshot.environment)
                        .with_subscriber(self.logger_subscriber(logdir).1)
                        .await;
                    if let Err(err) = res {
                        error!("{err}");
                    }
                }
            }
            Service::Bundle(_) | Service::Virtual(_) => todo!(),
        }
    }

    pub fn logger_subscriber(
        &self,
        logdir: &Path,
    ) -> (
        FileLogWriterHandle,
        tracing_subscriber::fmt::SubscriberBuilder<
            tracing_subscriber::fmt::format::DefaultFields,
            tracing_subscriber::fmt::format::Format,
            LevelFilter,
            impl Fn() -> flexi_logger::writers::ArcFileLogWriter,
        >,
    ) {
        let (file_writer, fw_handle) = FileLogWriter::builder(
            FileSpec::default()
                .directory(logdir.join(self.node.name()))
                .basename(self.node.name().to_owned()),
        )
        .rotate(
            Criterion::Size(1024 * 512),
            Naming::Numbers,
            Cleanup::KeepCompressedFiles(5),
        )
        .append()
        .write_mode(WriteMode::Async)
        .try_build_with_handle()
        .unwrap();

        (
            fw_handle,
            FmtSubscriber::builder()
                .with_level(false)
                .with_target(false)
                .with_writer(move || file_writer.clone())
                .with_max_level(LevelFilter::INFO),
        )
    }
}
