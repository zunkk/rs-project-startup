use std::sync::Arc;

use axum::http::HeaderMap;
use chrono::Duration;
use rand::Rng;
use rand::distr::Alphanumeric;
use serde::{Deserialize, Serialize};
use sidecar::prelude::*;
use utoipa::OpenApi;

use crate::core::core::Core;
use crate::core::model::user::Role;
use crate::core::model::user_auth::AuthType;
use crate::kit::context::Context;
use crate::kit::jwt;
use crate::kit::response::Response;

/// User module OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
    paths(register, login, refresh_token),
    components(
        schemas(
            RegisterReq,
            RegisterRes,
            Response<RegisterRes>,
            LoginReq,
            LoginRes,
            Response<LoginRes>,
            RefreshTokenRes,
            Response<RefreshTokenRes>,
        )
    ),
    tags((name = "user", description = "User management related APIs"))
)]
pub struct UserApiDoc;

/// User registration request body
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RegisterReq {
    /// Authentication method
    /// username: auth_id is username, auth_token is password
    #[schema(example = "Username")]
    pub auth_type: AuthType,
    /// External account unique identifier
    #[schema(example = "admin")]
    pub auth_id: String,
    /// Authentication credentials
    #[schema(example = "admin")]
    pub auth_token: String,
    /// Target role
    #[schema(example = "Admin")]
    pub role: Role,
    /// User nickname, auto-generated if not provided
    #[schema(nullable = false, example = "admin")]
    pub nickname: Option<String>,
    /// User description information, default is empty
    #[schema(nullable = false, example = "admin")]
    pub desc: Option<String>,
}

/// User registration response body
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RegisterRes {
    /// Unique identifier of the newly created user
    pub user_id: String,
}

/// User registration endpoint
#[utoipa::path(
    tag = "user",
    operation_id = "user_register",
    post,
    path = "/register",
    summary = "Register new user",
    description = "Create a new user with the specified role and return the user unique identifier.",
    request_body = RegisterReq,
    responses((status = 200, description = "Registration successful", body = Response<RegisterRes>))
)]
pub async fn register(
    state: Arc<Core>,
    _ctx: Context,
    _headers: HeaderMap,
    req: RegisterReq,
) -> Result<RegisterRes> {
    let user_id = state
        .service
        .user
        .register(
            req.auth_type,
            req.auth_id,
            req.auth_token,
            req.role,
            req.nickname.unwrap_or(format!("user-{}", random_string(6))),
            req.desc.unwrap_or("".to_string()),
        )
        .await?;

    Ok(RegisterRes { user_id })
}

/// User login request parameters
#[derive(Debug, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
#[into_params(parameter_in = Query)]
pub struct LoginReq {
    /// Authentication method
    /// username: auth_id is username, auth_token is password
    #[param(example = "Username")]
    pub auth_type: AuthType,
    /// External account unique identifier
    #[param(example = "admin")]
    pub auth_id: String,
    /// Authentication credentials
    #[param(example = "admin")]
    pub auth_token: String,
}

/// User login response body
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LoginRes {
    /// Unique identifier of the logged-in user
    pub user_id: String,
    /// Issued JWT token
    pub jwt_token: String,
    /// Token expiration time (Unix timestamp, seconds)
    pub expired_time: i64,
}

/// User login endpoint
#[utoipa::path(
    tag = "user",
    operation_id = "user_login",
    get,
    path = "/login",
    summary = "Login with credentials",
    description = "Verify account credentials and return a usable JWT access token.",
    params(LoginReq),
    responses((status = 200, description = "Login successful", body = Response<LoginRes>))
)]
pub async fn login(
    state: Arc<Core>,
    _ctx: Context,
    _headers: HeaderMap,
    req: LoginReq,
) -> Result<LoginRes> {
    let user_id = state
        .service
        .user
        .login(req.auth_type, req.auth_id, req.auth_token)
        .await?;

    let (jwt_token, expired_time) = jwt::generate_with_hmac_key(
        &state.repo.cfg.http.jwt.token_hmac_key,
        Duration::from_std(state.repo.cfg.http.jwt.token_valid_duration.into())?,
        &user_id,
        (),
    )?;

    Ok(LoginRes {
        user_id,
        jwt_token,
        expired_time,
    })
}

/// Refresh JWT Token response body
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RefreshTokenRes {
    /// Unique identifier of the logged-in user
    pub user_id: String,
    /// Issued JWT token
    pub jwt_token: String,
    /// Token expiration time (Unix timestamp, seconds)
    pub expired_time: i64,
}

/// User refresh token endpoint
#[utoipa::path(
    tag = "user",
    operation_id = "user_refresh_token",
    get,
    path = "/refresh-token",
    summary = "User refresh JWT Token",
    description = "User logs in with a valid JWT token and returns a new JWT access token.",
    security(("bearer_auth" = [])),
    responses((status = 200, description = "Refresh successful", body = Response<RefreshTokenRes>))
)]
pub async fn refresh_token(
    state: Arc<Core>,
    ctx: Context,
    _headers: HeaderMap,
    _req: (),
) -> Result<RefreshTokenRes> {
    let (jwt_token, expired_time) = jwt::generate_with_hmac_key(
        &state.repo.cfg.http.jwt.token_hmac_key,
        Duration::from_std(state.repo.cfg.http.jwt.token_valid_duration.into())?,
        &ctx.user_id,
        (),
    )?;

    Ok(RefreshTokenRes {
        user_id: ctx.user_id,
        jwt_token,
        expired_time,
    })
}

/// Generate a random string of specified length, used for fallback nickname
fn random_string(len: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}
