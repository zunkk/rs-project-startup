use std::sync::Arc;

use tokio::sync::RwLock;

#[derive(Default, Clone, Debug)]
pub struct Context {
    pub user_id: String,
    pub log_fields: Arc<RwLock<Vec<(String, String)>>>,
    pub log_fields_on_error: Arc<RwLock<Vec<(String, String)>>>,
}

impl Context {
    pub async fn add_log_field(&self, key: impl Into<String>, value: impl Into<String>) {
        let mut log_fields = self.log_fields.write().await;
        log_fields.push((key.into(), value.into()));
    }

    pub async fn add_log_field_on_error(&self, key: impl Into<String>, value: impl Into<String>) {
        let mut log_fields_on_error = self.log_fields_on_error.write().await;
        log_fields_on_error.push((key.into(), value.into()));
    }
}
