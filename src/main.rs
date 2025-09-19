pub mod api;
pub mod cmd;
pub mod core;
pub mod kit;

use std::path::PathBuf;
use std::{env, fs};

use chrono::{DateTime, Local, SecondsFormat, Utc};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use sidecar::prelude::*;
use sidecar::repo::Repo;
use sidecar::{setup, version};

use crate::kit::config::Config;

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Parser)]
struct Cli {
    #[arg(long = "repo-root", value_name = "PATH")]
    repo_root: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Config {
        #[command(subcommand)]
        command: cmd::config::Cmd,
    },
    Ipc {
        #[command(subcommand)]
        command: cmd::ipc::Cmd,
    },
    Run(cmd::run::RunArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    setup::setup_libs()?;
    version::init(load_version_from_env());

    let cli = parse_cli();

    let Cli { repo_root, command } = cli;
    let repo_root = resolve_repo_root(repo_root)?;

    let v = version::current();
    let repo = Repo::<Config>::new(repo_root, v.app_name).await?;

    match command {
        Some(Commands::Run(args)) => args.run(repo).await,
        Some(Commands::Config { command }) => cmd::config::run(command, repo).await,
        Some(Commands::Ipc { command }) => cmd::ipc::run(command, repo).await,
        None => {
            println!("{} {}", v.app_name, v.version);
            println!("git_branch：{}", v.git_branch);
            println!("git_commit：{}", v.git_commit);
            println!("build_time：{}", v.build_time);

            Ok(())
        }
    }
}

fn parse_cli() -> Cli {
    let version = version::current();

    let long_version = format!(
        "{}\ngit_branch：{}\ngit_commit：{}\nbuild_time：{}",
        version.version, version.git_branch, version.git_commit, version.build_time
    );
    let long_version: &'static str = Box::leak(long_version.into_boxed_str());

    let cli = Cli::command()
        .name(version.app_name)
        .version(version.version)
        .author(version.app_authors)
        .about(version.app_desc)
        .long_version(long_version)
        .long_about(None);

    let matches = cli.get_matches();

    match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(err) => err.exit(),
    }
}

fn load_version_from_env() -> version::Version {
    let version = if let Some(v) = option_env!("APP_VERSION") {
        if v.is_empty() {
            env!("CARGO_PKG_VERSION")
        } else {
            v
        }
    } else {
        env!("CARGO_PKG_VERSION")
    };

    let build_time = if let Some(build_time) = option_env!("VERGEN_BUILD_TIMESTAMP") {
        if let Ok(utc_dt) = build_time.parse::<DateTime<Utc>>() {
            let local_dt: DateTime<Local> = utc_dt.with_timezone(&Local);
            Box::leak(
                local_dt
                    .to_rfc3339_opts(SecondsFormat::Secs, true)
                    .into_boxed_str(),
            )
        } else {
            build_time
        }
    } else {
        "unknown"
    };

    version::Version {
        app_name: env!("CARGO_PKG_NAME"),
        app_desc: option_env!("CARGO_PKG_DESCRIPTION")
            .filter(|value| !value.is_empty())
            .unwrap_or(env!("CARGO_PKG_NAME")),
        app_authors: env!("CARGO_PKG_AUTHORS"),
        version,
        git_branch: option_env!("VERGEN_GIT_BRANCH").unwrap_or("unknown"),
        git_commit: option_env!("VERGEN_GIT_SHA").unwrap_or("unknown"),
        build_time,
    }
}

fn resolve_repo_root(repo_root: Option<PathBuf>) -> Result<PathBuf> {
    let candidate = if let Some(repo_root) = repo_root {
        repo_root
    } else {
        env::current_dir().unwrap_or(PathBuf::from("./"))
    };

    fs::canonicalize(candidate).wrap_err("Failed to canonicalize repo_root")
}
