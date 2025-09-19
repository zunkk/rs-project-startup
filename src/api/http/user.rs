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

/// 用户模块 OpenAPI 文档
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
    tags((name = "user", description = "用户管理相关接口"))
)]
pub struct UserApiDoc;

/// 用户注册请求体
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RegisterReq {
    /// 登录方式
    /// username：auth_id为用户名，auth_token为密码
    #[schema(example = "Username")]
    pub auth_type: AuthType,
    /// 外部账号唯一标识
    #[schema(example = "admin")]
    pub auth_id: String,
    /// 登录凭证
    #[schema(example = "admin")]
    pub auth_token: String,
    /// 目标角色
    #[schema(example = "Admin")]
    pub role: Role,
    /// 用户昵称，未填写时自动生成
    #[schema(nullable = false, example = "admin")]
    pub nickname: Option<String>,
    /// 用户描述信息，默认为空
    #[schema(nullable = false, example = "admin")]
    pub desc: Option<String>,
}

/// 用户注册响应体
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RegisterRes {
    /// 新创建用户的唯一标识
    pub user_id: String,
}

/// 用户注册接口
#[utoipa::path(
    tag = "user",
    operation_id = "user_register",
    post,
    path = "/register",
    summary = "注册新用户",
    description = "创建一个具备指定角色的新用户，并返回用户唯一标识。",
    request_body = RegisterReq,
    responses((status = 200, description = "注册成功", body = Response<RegisterRes>))
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

/// 用户登录请求参数
#[derive(Debug, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
#[into_params(parameter_in = Query)]
pub struct LoginReq {
    /// 登录方式
    /// username：auth_id为用户名，auth_token为密码
    #[param(example = "Username")]
    pub auth_type: AuthType,
    /// 外部账号唯一标识
    #[param(example = "admin")]
    pub auth_id: String,
    /// 登录凭证
    #[param(example = "admin")]
    pub auth_token: String,
}

/// 用户登录响应体
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LoginRes {
    /// 登录用户唯一标识
    pub user_id: String,
    /// 颁发的 JWT 凭证
    pub jwt_token: String,
    /// 凭证过期时间（Unix 时间戳，秒）
    pub expired_time: i64,
}

/// 用户登录接口
#[utoipa::path(
    tag = "user",
    operation_id = "user_login",
    get,
    path = "/login",
    summary = "通过凭证登录",
    description = "校验账号凭证，返回可用的 JWT 访问令牌。",
    params(LoginReq),
    responses((status = 200, description = "登录成功", body = Response<LoginRes>))
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

/// 刷新JWT Token响应体
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RefreshTokenRes {
    /// 登录用户唯一标识
    pub user_id: String,
    /// 颁发的 JWT 凭证
    pub jwt_token: String,
    /// 凭证过期时间（Unix 时间戳，秒）
    pub expired_time: i64,
}

/// 用户刷新Token
#[utoipa::path(
    tag = "user",
    operation_id = "user_refresh_token",
    get,
    path = "/refresh-token",
    summary = "用户刷新JWT Token",
    description = "用户登录后携带旧的有效JWT token，返回新的 JWT 访问令牌。",
    security(("bearer_auth" = [])),
    responses((status = 200, description = "刷新成功", body = Response<RefreshTokenRes>))
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

/// 生成指定长度的随机字符串，用于兜底昵称
fn random_string(len: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}
