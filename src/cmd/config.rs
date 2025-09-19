use clap::{Args, Subcommand};
use sidecar::prelude::*;
use sidecar::repo::Repo;

use crate::kit::config::Config;

#[derive(Subcommand)]
pub enum Cmd {
    GenerateDefault(GenerateDefaultArgs),
    Check(CheckArgs),
    Show(ShowArgs),
}

pub async fn run(cmd: Cmd, repo: Repo<Config>) -> Result<()> {
    match cmd {
        Cmd::GenerateDefault(args) => args.run(repo).await,
        Cmd::Check(args) => args.run(repo).await,
        Cmd::Show(args) => args.run(repo).await,
    }
}

#[derive(Args)]
pub struct GenerateDefaultArgs {}

impl GenerateDefaultArgs {
    pub async fn run(self, repo: Repo<Config>) -> Result<()> {
        if repo.config_exists() {
            println!(
                "config file already exists: {}",
                repo.config_path().display()
            );
            return Ok(());
        }

        repo.save().await?;

        println!(
            "default config file generated: {}",
            repo.config_path().display()
        );

        Ok(())
    }
}

#[derive(Args)]
pub struct CheckArgs {}

impl CheckArgs {
    pub async fn run(self, mut repo: Repo<Config>) -> Result<()> {
        if !repo.config_exists() {
            return Ok(());
        }

        repo.reload().await?;

        Ok(())
    }
}

#[derive(Args)]
pub struct ShowArgs {}

impl ShowArgs {
    pub async fn run(self, repo: Repo<Config>) -> Result<()> {
        let cfg_data = toml::to_string(&repo.cfg)?;

        println!("config: \n");
        println!("{cfg_data}");

        Ok(())
    }
}
