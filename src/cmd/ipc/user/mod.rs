use clap::Subcommand;
use sidecar::prelude::*;

use super::client::IpcContext;

pub mod register;

#[derive(Subcommand)]
pub enum Cmd {
    Register(register::RegisterArgs),
}
pub async fn run(cmd: Cmd, ctx: IpcContext) -> Result<()> {
    match cmd {
        Cmd::Register(args) => register::run(args, ctx).await,
    }
}
