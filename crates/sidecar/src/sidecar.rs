use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::{Mutex, Notify, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::lifecycle::LifecycleManager;
use crate::prelude::*;

type ComponentHandle = Arc<dyn Component>;
type AppReadyFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

#[async_trait]
pub trait Component: Send + Sync {
    fn name(&self) -> &str;
    async fn start(&self) -> Result<()> {
        Ok(())
    }
    async fn stop(&self) -> Result<()> {
        Ok(())
    }
}

struct SidecarInner {
    lifecycle_manager: LifecycleManager,
    components: RwLock<Vec<ComponentHandle>>,
    no_block_app_ready_callbacks: Mutex<Vec<AppReadyFuture>>,
    block_app_ready_callbacks: Mutex<Vec<AppReadyFuture>>,
}

#[derive(Clone)]
pub struct Sidecar {
    pub current_component_name: String,
    inner: Arc<SidecarInner>,
}

impl Sidecar {
    pub fn new() -> Self {
        Sidecar {
            current_component_name: "".to_string(),
            inner: Arc::new(SidecarInner {
                lifecycle_manager: LifecycleManager::new(),
                components: RwLock::new(Vec::new()),
                no_block_app_ready_callbacks: Mutex::new(Vec::new()),
                block_app_ready_callbacks: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn with_component_name(&self, name: impl Into<String>) -> Self {
        let mut c = self.clone();
        c.current_component_name = name.into();
        c
    }

    pub async fn canceled(&self) -> Result<()> {
        self.inner.lifecycle_manager.canceled().await;
        Ok(())
    }

    pub async fn cancel(&self) -> Result<()> {
        self.inner
            .lifecycle_manager
            .cancel(&self.current_component_name)
            .await;
        Ok(())
    }

    pub async fn register_component<C>(&self, component: Arc<C>) -> Result<()>
    where
        C: Component + 'static,
    {
        let mut components = self.inner.components.write().await;
        let handle: ComponentHandle = component;
        components.push(handle);
        Ok(())
    }

    pub async fn register_app_ready_callback<F, Fut>(&self, callback: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.register_no_block_app_ready_callback(callback).await;
    }

    pub async fn register_no_block_app_ready_callback<F, Fut>(&self, callback: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let future = Box::pin(callback());
        let mut guard = self.inner.no_block_app_ready_callbacks.lock().await;
        guard.push(future);
    }

    pub async fn register_block_app_ready_callback<F, Fut>(&self, callback: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let future = Box::pin(callback());
        let mut guard = self.inner.block_app_ready_callbacks.lock().await;
        guard.push(future);
    }

    pub fn spawn_core_task<F>(&self, task_name: impl Into<String>, task: F) -> TaskHandle
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let component_name = self.current_component_name.clone();
        let task_name = task_name.into();
        let handle = TaskHandle::new();
        let cancel_token = handle.cancellation_token();
        let completion_handle = handle.clone();
        info!(component = ?component_name, task = ?task_name, "core task run");
        self.inner.lifecycle_manager.spawn_task(async move {
            let mut task = Box::pin(task);
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    info!(component = ?component_name, task = ?task_name, "core task cancelled");
                }
                _ = &mut task => {
                    info!(component = ?component_name, task = ?task_name, "core task down");
                }
            }
            completion_handle.mark_complete();
        });

        handle
    }

    pub fn spawn_scheduled_task<T, F, Fut>(
        &self,
        task_name: impl Into<String>,
        interval: Duration,
        state: T,
        task: F,
    ) -> TaskHandle
    where
        T: Clone + Send + Sync + 'static,
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        let component_name = self.current_component_name.clone();
        let task_name = task_name.into();
        let sidecar = self.clone();
        let handle = TaskHandle::new();
        let cancel_token = handle.cancellation_token();
        let completion_handle = handle.clone();

        self.inner.lifecycle_manager.spawn_task(async move {
            info!(
                component = ?component_name,
                task = ?task_name,
                interval = ?interval,
                "scheduled task run"
            );
            let mut ticker = tokio::time::interval(interval);

            loop {
                tokio::select! {
                    _ = sidecar.canceled() => {
                        info!(component = ?component_name, task = ?task_name, "scheduled task down");
                        break;
                    }
                    _ = cancel_token.cancelled() => {
                        info!(component = ?component_name, task = ?task_name, "scheduled task cancelled");
                        break;
                    }
                    _ = ticker.tick() => {
                        let fut = task(state.clone());
                        let result = fut.await;
                        if let Err(err) = result {
                            warn!(
                                component = ?component_name,
                                task = ?task_name,
                                error = ?err,
                                "scheduled task tick failed"
                            )
                        }
                    }
                }
            }

            completion_handle.mark_complete();
        });

        handle
    }

    pub async fn run(self) -> Result<()> {
        info!("components starting");
        let start_time = Instant::now();
        let active_components = self.start_components().await?;
        let elapsed = start_time.elapsed();
        info!(elapsed = ?elapsed, "components started");

        for future in {
            let mut guard = self.inner.no_block_app_ready_callbacks.lock().await;
            guard.drain(..).collect::<Vec<_>>()
        } {
            tokio::spawn(future);
        }

        for future in {
            let mut guard = self.inner.block_app_ready_callbacks.lock().await;
            guard.drain(..).collect::<Vec<_>>()
        } {
            future.await;
        }

        info!("app is running");
        self.inner.lifecycle_manager.wait().await;

        info!("components stopping");
        let start_time = Instant::now();
        self.stop_components(active_components).await?;
        let elapsed = start_time.elapsed();
        info!(elapsed = ?elapsed, "components stopped");
        self.inner.components.write().await.clear();

        info!("app down");
        Ok(())
    }

    async fn start_components(&self) -> Result<Vec<ComponentHandle>> {
        let handles = {
            let components = self.inner.components.read().await;
            components.clone()
        };

        let mut started = Vec::new();

        for component in &handles {
            let name = component.name().to_string();
            let start_time = Instant::now();
            info!(component = ?name, "component starting");
            if let Err(err) = component
                .start()
                .await
                .wrap_err_with(|| format!("Failed to start component[{name}] "))
            {
                if let Err(stop_err) = self.stop_components(started).await {
                    error!(error = ?stop_err, "rollback components failed after start error");
                }
                return Err(err);
            }
            info!(component = ?name, elapsed = ?start_time.elapsed(), "component started");
            started.push(component.clone());
        }

        Ok(handles)
    }

    async fn stop_components(&self, handles: Vec<ComponentHandle>) -> Result<()> {
        for component in handles.into_iter().rev() {
            let name = component.name().to_string();
            let start_time = Instant::now();
            info!(component = ?name, "component stopping");
            component
                .stop()
                .await
                .wrap_err_with(|| format!("Failed to stop component[{name}] "))?;
            info!(component = ?name, elapsed = ?start_time.elapsed(), "component stopped");
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct TaskHandle {
    inner: Arc<TaskHandleInner>,
}

struct TaskHandleInner {
    cancel_token: CancellationToken,
    completed: AtomicBool,
    completion_notify: Notify,
}

impl TaskHandleInner {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            cancel_token: CancellationToken::new(),
            completed: AtomicBool::new(false),
            completion_notify: Notify::new(),
        })
    }

    fn mark_complete(&self) {
        self.completed.store(true, Ordering::SeqCst);
        self.completion_notify.notify_waiters();
    }
}

impl TaskHandle {
    fn new() -> Self {
        TaskHandle {
            inner: TaskHandleInner::new(),
        }
    }

    pub async fn cancel(&self, timeout: Duration) -> bool {
        self.inner.cancel_token.cancel();

        if self.inner.completed.load(Ordering::SeqCst) {
            return true;
        }

        match tokio::time::timeout(timeout, self.inner.completion_notify.notified()).await {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    fn cancellation_token(&self) -> CancellationToken {
        self.inner.cancel_token.clone()
    }

    fn mark_complete(&self) {
        self.inner.mark_complete();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::time::{Duration, sleep};
    use tracing::info;

    use super::*;
    use crate::log;

    #[tokio::test]
    async fn test_sidecar_shutdown() -> Result<()> {
        log::default_setup();
        let sidecar = Sidecar::new();

        tokio::spawn({
            let sidecar = sidecar.clone();
            async move {
                tokio::select! {
                    _ = sleep(Duration::from_secs(1)) => {
                        sidecar.cancel().await.unwrap();
                    }
                    _ = sidecar.canceled() => {
                    }
                }
            }
        });

        info!("robot is on");
        sidecar.run().await?;
        info!("robot is down");
        Ok(())
    }

    #[tokio::test]
    async fn test_app_ready_callbacks() -> Result<()> {
        log::default_setup();
        let sidecar = Sidecar::new();
        let counter = Arc::new(AtomicUsize::new(0));

        sidecar
            .register_no_block_app_ready_callback({
                let sidecar = sidecar.clone();
                let counter = counter.clone();
                move || async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    sidecar.cancel().await.unwrap();
                }
            })
            .await;

        info!("robot is on with callbacks");
        sidecar.run().await?;
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        info!("robot is down");
        Ok(())
    }

    #[derive(Clone)]
    struct TrackingComponent {
        name: &'static str,
        sidecar: Sidecar,
        start_count: Arc<AtomicUsize>,
        stop_count: Arc<AtomicUsize>,
    }

    impl TrackingComponent {
        async fn new(sidecar: &Sidecar) -> Result<Arc<Self>> {
            let component = Arc::new(TrackingComponent {
                name: "tracking",
                sidecar: sidecar.with_component_name("tracking"),
                start_count: Arc::new(AtomicUsize::new(0)),
                stop_count: Arc::new(AtomicUsize::new(0)),
            });

            sidecar.register_component(component.clone()).await?;

            Ok(component)
        }
    }

    #[async_trait]
    impl Component for TrackingComponent {
        fn name(&self) -> &str {
            self.name
        }

        async fn start(&self) -> Result<()> {
            self.start_count.fetch_add(1, Ordering::SeqCst);

            self.sidecar.spawn_core_task("test_task", {
                let sidecar = self.sidecar.clone();
                async move {
                    tokio::select! {
                        _ = sleep(Duration::from_secs(1)) => {
                            sidecar.cancel().await.unwrap();
                        }
                        _ = sidecar.canceled() => {
                        }
                    }
                }
            });
            Ok(())
        }

        async fn stop(&self) -> Result<()> {
            self.stop_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_component_lifecycle() -> Result<()> {
        log::default_setup();
        let sidecar = Sidecar::new();

        let component = TrackingComponent::new(&sidecar).await?;

        sidecar.run().await?;

        assert_eq!(
            component.start_count.load(Ordering::SeqCst),
            1,
            "Component not started"
        );
        assert_eq!(
            component.stop_count.load(Ordering::SeqCst),
            1,
            "Component not stopped"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_core_task_handle_cancel() -> Result<()> {
        log::default_setup();
        let sidecar = Sidecar::new().with_component_name("core_handle");

        let flag = Arc::new(RwLock::new(false));
        let flag_clone = flag.clone();

        let handle = sidecar.spawn_core_task("core_cancel_task", async move {
            {
                let mut guard = flag_clone.write().await;
                *guard = true;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(*flag.read().await, "Core task not started");

        let cancelled = handle.cancel(Duration::from_millis(100)).await;
        assert!(
            cancelled,
            "Core task cancellation not completed within timeout"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_spawn_scheduled_task_runs_multiple_times() -> Result<()> {
        log::default_setup();
        let sidecar = Sidecar::new().with_component_name("interval");

        let counter = Arc::new(RwLock::new(0u32));

        sidecar.spawn_scheduled_task(
            "scheduled_task",
            Duration::from_millis(10),
            counter.clone(),
            |counter| async move {
                let mut guard = counter.write().await;
                *guard += 1;
                Ok(())
            },
        );

        tokio::time::sleep(Duration::from_millis(35)).await;
        sidecar.cancel().await?;
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(
            *counter.read().await >= 2,
            "Scheduled task not executed multiple times"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_spawn_scheduled_task_handle_cancel() -> Result<()> {
        log::default_setup();
        let sidecar = Sidecar::new().with_component_name("interval_handle");

        let counter = Arc::new(RwLock::new(0u32));

        let handle = sidecar.spawn_scheduled_task(
            "scheduled_cancel",
            Duration::from_secs(1),
            counter.clone(),
            |counter| async move {
                let mut guard = counter.write().await;
                *guard += 1;
                Ok(())
            },
        );

        tokio::time::sleep(Duration::from_millis(20)).await;
        let cancelled = handle.cancel(Duration::from_millis(100)).await;
        assert!(
            cancelled,
            "Scheduled task cancellation not completed within timeout"
        );

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            *counter.read().await <= 1,
            "Scheduled task continues to execute after cancellation"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_spawn_scheduled_task_handles_error() -> Result<()> {
        log::default_setup();
        let sidecar = Sidecar::new().with_component_name("interval_error");

        #[derive(Clone)]
        struct TaskState {
            success_counter: Arc<RwLock<u32>>,
            error_counter: Arc<RwLock<u32>>,
            toggle: Arc<AtomicBool>,
        }

        let state = TaskState {
            success_counter: Arc::new(RwLock::new(0u32)),
            error_counter: Arc::new(RwLock::new(0u32)),
            toggle: Arc::new(AtomicBool::new(false)),
        };

        sidecar.spawn_scheduled_task(
            "scheduled_task_error",
            Duration::from_millis(10),
            Arc::new(state.clone()),
            |state| async move {
                let previous_value = state.toggle.fetch_not(Ordering::SeqCst);
                if previous_value {
                    let mut guard = state.success_counter.write().await;
                    *guard += 1;
                    Ok(())
                } else {
                    let mut guard = state.error_counter.write().await;
                    *guard += 1;
                    eyre::bail!("scheduled task expected error")
                }
            },
        );

        tokio::time::sleep(Duration::from_millis(45)).await;
        sidecar.cancel().await?;
        tokio::time::sleep(Duration::from_millis(10)).await;

        let success_count = *state.success_counter.read().await;
        let error_count = *state.error_counter.read().await;

        assert!(
            success_count >= 1,
            "Scheduled task success branch not executed"
        );
        assert!(
            error_count >= 1,
            "Scheduled task error branch not triggered"
        );

        Ok(())
    }
}
