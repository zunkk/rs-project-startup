use std::io;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use config::{Case, Config, Environment, File, FileFormat};
use serde::Serialize;
use tokio::fs;

use crate::prelude::*;

#[async_trait]
pub trait IConfig:
    Serialize + for<'a> serde::Deserialize<'a> + Default + Clone + Send + Sync
{
    async fn init(&mut self, repo_root: PathBuf) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct Repo<C: IConfig> {
    pub app_name: String,
    pub root: PathBuf,
    pub cfg: C,
}

impl<C: IConfig> Repo<C> {
    pub async fn new(repo_root: impl AsRef<Path>, app_name: impl Into<String>) -> Result<Self> {
        let root = repo_root.as_ref().to_path_buf();
        let app_name = app_name.into();
        let cfg = C::default();

        let mut repo = Self {
            app_name,
            root: root.clone(),
            cfg,
        };
        repo.reload().await?;
        repo.cfg.init(root).await?;
        Ok(repo)
    }

    fn config_stem(&self) -> PathBuf {
        self.root.join("config")
    }

    pub fn config_path(&self) -> PathBuf {
        self.config_stem().with_extension("toml")
    }

    pub fn config_exists(&self) -> bool {
        self.config_path().exists()
    }

    pub fn ipc_file_path(&self) -> PathBuf {
        self.root.join("ipc.sock")
    }

    pub fn pid_file_path(&self) -> PathBuf {
        self.root.join("process.pid")
    }

    pub async fn write_pid(&self) -> Result<()> {
        fs::write(&self.pid_file_path(), std::process::id().to_string()).await?;
        Ok(())
    }

    pub async fn remove_pid(&self) -> Result<()> {
        let path = self.pid_file_path();
        if path.exists() {
            fs::remove_file(path).await?;
        }
        Ok(())
    }

    pub async fn reload(&mut self) -> Result<()> {
        dotenv::from_path(self.root.join(".env")).ok();

        match fs::create_dir_all(&self.root).await {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => {
                return Err(e.into());
            }
        };

        let default_cfg = Config::try_from(&C::default())?;
        let env_prefix = self.app_name.to_lowercase().replace("-", "_");
        let config_stem = self.config_stem();
        let config_stem = config_stem.to_string_lossy().into_owned();
        self.cfg = Config::builder()
            .add_source(default_cfg)
            .add_source(
                File::with_name(&config_stem)
                    .format(FileFormat::Toml)
                    .required(false),
            )
            .add_source(
                Environment::with_prefix(&env_prefix)
                    .convert_case(Case::Snake)
                    .separator("_"),
            )
            .build()?
            .try_deserialize::<C>()?;

        Ok(())
    }

    pub async fn save(&self) -> Result<()> {
        let config_path = self.config_path();
        if let Some(parent) = config_path.parent() {
            if let Err(e) = fs::create_dir_all(parent).await {
                if e.kind() != io::ErrorKind::AlreadyExists {
                    return Err(e.into());
                }
            }
        }
        let cfg_data = toml::to_string(&self.cfg)?;
        fs::write(&config_path, cfg_data).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};
    use serial_test::serial;
    use tempfile::tempdir;

    use super::*;

    #[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
    struct TestConfig {
        #[serde(default)]
        value: u32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct DefaultOnlyConfig {
        value: u32,
    }

    impl Default for DefaultOnlyConfig {
        fn default() -> Self {
            Self { value: 11 }
        }
    }

    struct EnvVarGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prev = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, prev }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(prev) = &self.prev {
                unsafe {
                    std::env::set_var(self.key, prev);
                }
            } else {
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    #[async_trait]
    impl IConfig for TestConfig {
        async fn init(&mut self, repo_root: PathBuf) -> Result<()> {
            self.value += 1;
            Ok(())
        }
    }

    #[async_trait]
    impl IConfig for DefaultOnlyConfig {
        async fn init(&mut self, repo_root: PathBuf) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_new_initializes_config() -> Result<()> {
        let tmp = tempdir()?;
        let repo = Repo::<TestConfig>::new(tmp.path(), "demo-app").await?;

        assert_eq!(repo.cfg.value, 1);
        assert!(repo.root.exists());

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_save_persists_config() -> Result<()> {
        let tmp = tempdir()?;
        let mut repo = Repo::<TestConfig>::new(tmp.path(), "demo-app").await?;

        repo.cfg.value = 42;
        repo.save().await?;

        let saved = tokio::fs::read_to_string(repo.root.join("config.toml")).await?;
        assert!(saved.contains("value = 42"));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_reload_reads_saved_config() -> Result<()> {
        let tmp = tempdir()?;
        let mut repo = Repo::<TestConfig>::new(tmp.path(), "demo-app").await?;

        repo.cfg.value = 7;
        repo.save().await?;

        repo.cfg.value = 0;
        repo.reload().await?;

        assert_eq!(repo.cfg.value, 7);

        Ok(())
    }

    #[tokio::test]
    async fn test_default_used_when_no_config_file() -> Result<()> {
        let tmp = tempdir()?;
        let repo = Repo::<DefaultOnlyConfig>::new(tmp.path(), "default-app").await?;

        assert_eq!(repo.cfg.value, 11);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_environment_overrides_file_and_default() -> Result<()> {
        let tmp = tempdir()?;
        let _guard = EnvVarGuard::set("DEMO_APP_VALUE", "41");
        let mut repo = Repo::<TestConfig>::new(tmp.path(), "demo-app").await?;

        repo.cfg.value = 10;
        repo.save().await?;

        repo.cfg.value = 0;
        repo.reload().await?;

        assert_eq!(repo.cfg.value, 41);

        Ok(())
    }
}
