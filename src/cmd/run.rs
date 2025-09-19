use std::sync::Arc;

use clap::Args;
use sidecar::prelude::*;
use sidecar::repo::Repo;
use sidecar::sidecar::Sidecar;
use sidecar::{log, version};
use tracing::{info, warn};

use crate::api::http::server::Server;
use crate::core::core::Core;
use crate::kit::config::Config;

pub struct App {
    _core: Arc<Core>,
    _http_server: Arc<Server>,
}

impl App {
    pub async fn new(sidecar: Sidecar, repo: Repo<Config>) -> Result<Self> {
        // build components

        let core = Core::new(sidecar.clone(), repo.clone()).await?;

        let http_server = Server::new(sidecar.clone(), repo.clone(), core.clone()).await?;

        Ok(App {
            _core: core,
            _http_server: http_server,
        })
    }
}

#[derive(Args)]
pub struct RunArgs {}

impl RunArgs {
    pub async fn run(self, repo: Repo<Config>) -> Result<()> {
        let _log_guard = log::setup(
            repo.cfg.log.level,
            Some(repo.root.join("logs")),
            repo.cfg.log.max_log_files,
        );

        let sidecar = Sidecar::new();

        let _app = App::new(sidecar.clone(), repo.clone()).await?;

        sidecar
            .register_block_app_ready_callback({
                let repo = repo.clone();
                move || async move {
                    if let Err(e) = repo.write_pid().await {
                        warn!("failed to write pid file: {}", e);
                    }
                    let v = version::current();
                    info!("repo_root: {}", repo.root.display());
                    info!("{} version: {}", v.app_name, v.version);
                    info!("git_branch：{}", v.git_branch);
                    info!("git_commit：{}", v.git_commit);
                    info!("build_time：{}", v.build_time);
                }
            })
            .await;

        sidecar.run().await?;

        if let Err(e) = repo.remove_pid().await {
            warn!("failed to remove pid file: {}", e);
        }

        Ok(())
    }
}
