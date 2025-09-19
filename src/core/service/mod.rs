use std::sync::Arc;

use async_trait::async_trait;
use sidecar::prelude::*;
use sidecar::repo::Repo;
use sidecar::sidecar::{Component, Sidecar};

use crate::core::db::DB;
use crate::kit::config::Config;

pub mod user;

pub struct Service {
    sidecar: Sidecar,
    _repo: Repo<Config>,
    pub db: Arc<DB>,
    pub user: Arc<user::Service>,
}

impl Service {
    pub async fn new(sidecar: Sidecar, repo: Repo<Config>, db: Arc<DB>) -> Result<Arc<Self>> {
        let user_service = user::Service::new(sidecar.clone(), repo.clone(), db.clone()).await?;

        let service = Arc::new(Self {
            sidecar: sidecar.with_component_name("service"),
            _repo: repo,
            db,
            user: user_service,
        });

        sidecar.register_component(service.clone()).await?;

        Ok(service)
    }
}

#[async_trait]
impl Component for Service {
    fn name(&self) -> &str {
        &self.sidecar.current_component_name
    }

    async fn start(&self) -> Result<()> {
        self.user.create_tables().await?;

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }
}
