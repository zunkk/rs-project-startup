use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::IndexCreateStatement;
use sea_orm::{ConnectOptions, Database, ExecResult, Schema, Statement};
use sidecar::prelude::*;
use sidecar::repo::Repo;
use sidecar::sidecar::{Component, Sidecar};
use tokio::sync::RwLock;
use tracing::info;

use crate::kit::config::Config;
use crate::kit::error::Error;

pub struct DB {
    sidecar: Sidecar,
    repo: Repo<Config>,
    connection: RwLock<Option<DatabaseConnection>>,
}

impl DB {
    pub async fn new(sidecar: Sidecar, repo: Repo<Config>) -> Result<Arc<Self>> {
        let db = Arc::new(Self {
            sidecar: sidecar.with_component_name("db"),
            repo,
            connection: RwLock::new(None),
        });

        sidecar.register_component(db.clone()).await?;

        Ok(db)
    }

    fn dsn(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}?sslmode={}&options=--search_path%3d{}%20-c%20client_min_messages%3dERROR",
            self.repo.cfg.db.username,
            self.repo.cfg.db.password,
            self.repo.cfg.db.host,
            self.repo.cfg.db.port,
            self.repo.cfg.db.database,
            self.repo.cfg.db.ssl_mode,
            self.repo.cfg.db.schema,
        )
    }

    pub async fn get_connection(&self) -> Result<DatabaseConnection> {
        let guard = self.connection.read().await;
        if let Some(connection) = guard.as_ref() {
            return Ok(connection.clone());
        }
        Err(Error::DBConnectionNotInitialized.into())
    }

    pub async fn exec_str_sql(&self, sql: &str) -> Result<ExecResult> {
        let conn = self.get_connection().await?;

        self.exec_statement(Statement::from_string(
            conn.get_database_backend(),
            sql.to_owned(),
        ))
        .await
    }

    pub async fn exec_statement(&self, statement: Statement) -> Result<ExecResult> {
        let conn = self.get_connection().await?;
        let res = conn.execute_raw(statement).await?;
        Ok(res)
    }

    pub async fn create_table<M: EntityTrait>(
        &self,
        create_index_statements: Vec<IndexCreateStatement>,
    ) -> Result<()> {
        let conn = self.get_connection().await?;
        let m = M::default();

        let database_backend = conn.get_database_backend();

        let schema = Schema::new(database_backend);

        let statement = schema.create_table_from_entity(m);
        let ddl = database_backend.build(&statement);

        let created = match self.exec_statement(ddl).await {
            Ok(_) => true,
            Err(err) => {
                if err.to_string().contains("already exists") {
                    false
                } else {
                    return Err(err);
                }
            }
        };

        for create_index_statement in create_index_statements {
            self.exec_statement(database_backend.build(&create_index_statement))
                .await?;
        }

        if created {
            info!(table = m.table_name(), "table created");
        }

        Ok(())
    }
}

#[async_trait]
impl Component for DB {
    fn name(&self) -> &str {
        &self.sidecar.current_component_name
    }

    async fn start(&self) -> Result<()> {
        if !self.repo.cfg.db.enable {
            let mut guard = self.connection.write().await;
            guard.take();
            return Ok(());
        }
        let mut opts = ConnectOptions::new(self.dsn());
        opts.sqlx_logging(self.repo.cfg.db.log_sql);
        let connection = Database::connect(opts)
            .await
            .wrap_err("Connect to database failed")?;

        {
            let mut guard = self.connection.write().await;
            *guard = Some(connection.clone());
        }

        info!(dsn = ?self.dsn(), "db connected");

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let mut guard = self.connection.write().await;
        if let Some(connection) = guard.take() {
            connection.close().await?;
        }

        Ok(())
    }
}
