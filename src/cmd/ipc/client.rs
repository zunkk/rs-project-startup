use std::path::PathBuf;

use reqwest_middleware::ClientBuilder;
use sidecar::prelude::*;

use crate::api::http::client::apis::configuration;
use crate::api::http::client::apis::system_api;
use crate::api::http::client::apis::system_api::PingParams;

#[derive(Clone)]
pub struct IpcContext {
    pub configuration: configuration::Configuration,
}

impl IpcContext {
    pub fn new(socket_path: PathBuf) -> Result<Self> {
        let display_path = socket_path.display().to_string();
        let http_client = reqwest::Client::builder()
            .unix_socket(socket_path)
            .build()
            .wrap_err_with(|| format!("Failed to build ipc client: {}", display_path))?;
        let client = ClientBuilder::new(http_client).build();

        let mut configuration = configuration::Configuration::new();
        configuration.base_path = "http://localhost".to_string();
        configuration.client = client;

        Ok(Self { configuration })
    }

    pub async fn ping(&self) -> Result<()> {
        system_api::ping(&self.configuration, PingParams {
            content: Some("ping".to_string()),
        })
        .await?;

        Ok(())
    }
}
