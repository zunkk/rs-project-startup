use clap::Subcommand;
use sidecar::prelude::*;
use sidecar::repo::Repo;

use crate::kit::config::Config;

mod client;
mod user;

#[derive(Subcommand)]
pub enum Cmd {
    #[command(subcommand)]
    User(user::Cmd),
}
pub async fn run(cmd: Cmd, repo: Repo<Config>) -> Result<()> {
    let socket_path = repo.ipc_file_path();
    ensure!(
        socket_path.exists(),
        "IPC not exists, app is not running: {}",
        socket_path.display()
    );

    let ctx = client::IpcContext::new(socket_path)?;
    ctx.ping()
        .await
        .wrap_err("Failed to ping IPC, app is not running")?;

    match cmd {
        Cmd::User(user_cmd) => user::run(user_cmd, ctx).await,
    }
}
