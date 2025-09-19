use clap::Args;
use sidecar::prelude::*;

use super::super::client::IpcContext;
use crate::api::http::client::apis;
use crate::api::http::client::apis::user_api::{self, UserRegisterParams};
use crate::api::http::client::models;

#[derive(Args)]
pub struct RegisterArgs {
    #[arg(long, value_parser = parse_role, help = "Target role (admin; manager; user)")]
    role: models::Role,
    #[arg(long, value_parser = parse_auth_type, help = "Authentication method (username)")]
    auth_type: models::AuthType,
    #[arg(long, help = "External account unique identifier")]
    auth_id: String,
    #[arg(long, help = "Authentication credentials, e.g., password")]
    auth_token: String,
    #[arg(long, help = "User nickname, optional")]
    name: Option<String>,
    #[arg(long, help = "User description information, optional")]
    desc: Option<String>,
}

fn parse_auth_type(value: &str) -> std::result::Result<models::AuthType, String> {
    match value.to_ascii_lowercase().as_str() {
        "username" => Ok(models::AuthType::Username),
        _ => Err(format!("Invalid auth type: {value}")),
    }
}

fn parse_role(value: &str) -> std::result::Result<models::Role, String> {
    match value.to_ascii_lowercase().as_str() {
        "admin" => Ok(models::Role::Admin),
        "manager" => Ok(models::Role::Manager),
        "user" => Ok(models::Role::User),
        other => Err(format!("Invalid role: {other}")),
    }
}

pub async fn run(args: RegisterArgs, ctx: IpcContext) -> Result<()> {
    let RegisterArgs {
        auth_type,
        auth_id,
        auth_token,
        role,
        name,
        desc,
    } = args;

    let response = user_api::user_register(&ctx.configuration, UserRegisterParams {
        register_req: models::RegisterReq {
            auth_type,
            auth_id,
            auth_token,
            role,
            nickname: name,
            desc,
        },
    })
    .await
    .map_err(|err| match err {
        apis::Error::ResponseError(resp) => eyre!(
            "Request failed，status code: {}，body: {}",
            resp.status,
            resp.content
        ),
        other => eyre!(other),
    })?;

    ensure!(
        response.code == 0,
        "Request api failed code: {}，msg: {}",
        response.code,
        response.msg
    );

    let data = response
        .data
        .map(|boxed| *boxed)
        .ok_or_else(|| eyre!("Not found data"))?;

    println!("user registered，user_id: {}", data.user_id);

    Ok(())
}
