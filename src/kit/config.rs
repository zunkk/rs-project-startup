use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sidecar::prelude::*;
use sidecar::repo::IConfig;
use tracing::Level;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub db: DB,
    pub http: HTTP,
    pub log: Log,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db: DB {
                enable: false,
                host: "127.0.0.1".into(),
                port: 5432,
                username: "zunkk".into(),
                password: "zunkk".into(),
                database: "model".into(),
                schema: "public".into(),
                ssl_mode: "disable".into(),
                log_sql: false,
            },
            http: HTTP {
                enable: false,
                port: 8080,
                swagger: Swagger {
                    enable: true,
                    host: "http://127.0.0.1".to_string(),
                },
                jwt: JWT {
                    token_valid_duration: Duration::from_secs(3 * 24 * 60 * 60),
                    token_hmac_key: "rs-project-startup-hmac-key@2509".to_string(),
                },
            },
            log: Log {
                level: Level::DEBUG,
                max_log_files: 14,
            },
        }
    }
}

#[async_trait]
impl IConfig for Config {
    async fn init(&mut self, _repo_root: PathBuf) -> Result<()> {
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DB {
    pub enable: bool,
    pub host: String,
    pub port: u64,
    pub username: String,
    pub password: String,
    pub database: String,
    pub schema: String,
    pub ssl_mode: String,
    pub log_sql: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Swagger {
    pub enable: bool,
    pub host: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JWT {
    #[serde(with = "humantime_serde")]
    pub token_valid_duration: Duration,
    pub token_hmac_key: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HTTP {
    pub enable: bool,
    pub port: u64,
    pub swagger: Swagger,
    pub jwt: JWT,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Log {
    #[serde(with = "level_serde")]
    pub level: Level,
    pub max_log_files: u64,
}

mod level_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    use super::*;

    pub fn serialize<S>(level: &Level, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&level.as_str().to_lowercase())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Level, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Level::from_str(&raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_serde_round_trip() {
        let log = Log {
            level: Level::INFO,
            max_log_files: 7,
        };

        let json = serde_json::to_string(&log).expect("Failed to serialize log configuration");
        assert!(json.contains("\"info\""));

        let parsed: Log =
            serde_json::from_str(&json).expect("Failed to deserialize log configuration");
        assert_eq!(parsed.level, Level::INFO);
        assert_eq!(parsed.max_log_files, 7);
    }
}
