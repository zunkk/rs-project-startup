use std::sync::Arc;

use sidecar::prelude::*;
use sidecar::repo::Repo;
use sidecar::sidecar::Sidecar;

use crate::core::db::DB;
use crate::core::service::Service;
use crate::kit::config::Config;

pub struct Core {
    pub sidecar: Sidecar,
    pub repo: Repo<Config>,

    pub db: Arc<DB>,
    pub service: Arc<Service>,
}

impl Core {
    pub async fn new(sidecar: Sidecar, repo: Repo<Config>) -> Result<Arc<Self>> {
        let db = DB::new(sidecar.clone(), repo.clone()).await?;
        let service = Service::new(sidecar.clone(), repo.clone(), db.clone()).await?;

        Ok(Arc::new(Core {
            sidecar: sidecar.with_component_name("core"),
            repo,
            db,
            service,
        }))
    }
}
