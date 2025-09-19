use tokio::select;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::info;

pub struct LifecycleManager {
    task_tracker: TaskTracker,
    cancel_signal_cancellation_token: CancellationToken,
}

impl LifecycleManager {
    pub fn new() -> LifecycleManager {
        LifecycleManager {
            task_tracker: TaskTracker::new(),
            cancel_signal_cancellation_token: CancellationToken::new(),
        }
    }

    pub async fn cancel(&self, from_component: &str) {
        info!("component[{from_component}] request cancel");
        self.cancel_signal_cancellation_token.cancel();
    }

    pub async fn canceled(&self) {
        self.cancel_signal_cancellation_token.cancelled().await
    }

    pub fn spawn_task<F>(&self, task: F)
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.task_tracker.spawn(task);
    }

    // only main thread should call this
    pub async fn wait(&self) {
        let (cancel_timeout_signal_sender, mut cancel_timeout_signal_receiver) =
            mpsc::channel::<()>(1);

        // listen cancel signal
        tokio::spawn({
            let cancel_signal_cancellation_token = self.cancel_signal_cancellation_token.clone();
            let task_tracker = self.task_tracker.clone();
            let mut sigterm = signal(SignalKind::terminate()).unwrap();
            let mut sigint = signal(SignalKind::interrupt()).unwrap();
            async move {
                select! {
                    _ = sigterm.recv() => {
                        cancel_signal_cancellation_token.cancel();
                        info!(signal = "SIGTERM", "receive cancel signal");
                    },
                    _ = sigint.recv() => {
                        cancel_signal_cancellation_token.cancel();
                        info!(signal = "SIGINT", "receive cancel signal");
                    },
                    _ = cancel_signal_cancellation_token.cancelled() => {
                        info!(signal = "component", "receive cancel signal");
                    },
                }

                task_tracker.close();

                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    _ = cancel_timeout_signal_sender.send(()).await;
                })
            }
        });

        select! {
            _ = cancel_timeout_signal_receiver.recv() => {
                // timeout
                info!("component tasks cancel timeout, will force cancel");
            }
            _ = self.task_tracker.wait() => {
                // wait all task down
                info!("all component tasks down");
            }
        }
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}
