use std::collections::BTreeMap;
use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::{
    Router,
    extract::{
        ConnectInfo, FromRequestParts, Json, OriginalUri, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, header, request::Parts},
    response::{IntoResponse, Response as AxumResponse},
    routing::{MethodRouter, get, post},
};
use axum_client_ip::{
    CloudFrontViewerAddress, FlyClientIp, RightmostForwarded, RightmostXForwardedFor, TrueClientIp,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sidecar::prelude::*;
use sidecar::repo::Repo;
use sidecar::sidecar::{Component, Sidecar};
use strip_ansi_escapes::strip_str;
use tokio::fs;
use tokio::net::{TcpListener, UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{info, warn};
use utoipa::openapi::Components;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_swagger_ui::SwaggerUi;

use crate::api::http::user::{self, UserApiDoc};
use crate::core::core::Core;
use crate::kit::config::Config;
use crate::kit::context::Context;
use crate::kit::error::Error;
use crate::kit::jwt;
use crate::kit::response::Response;

#[derive(OpenApi)]
#[openapi(
    paths(ping),
    components(schemas(Response<String>)),
    tags((name = "system", description = "System related APIs")),
    modifiers(&BearerAuthAddon)
)]
pub struct ApiDoc;

struct BearerAuthAddon;

impl Modify for BearerAuthAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let mut components = openapi.components.take().unwrap_or_else(Components::new);

        if !components.security_schemes.contains_key("bearer_auth") {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }

        openapi.components = Some(components);
    }
}

pub fn base_openapi_doc() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi().nest("/api/v1/user", UserApiDoc::openapi())
}

#[derive(Clone)]
pub struct AppState {
    pub core: Arc<Core>,
    pub is_ipc: bool,
}

pub struct Server {
    sidecar: Sidecar,
    repo: Repo<Config>,

    core: Arc<Core>,
}

impl Server {
    pub async fn new(sidecar: Sidecar, repo: Repo<Config>, core: Arc<Core>) -> Result<Arc<Self>> {
        let server = Arc::new(Server {
            sidecar: sidecar.with_component_name("http-server"),
            repo,
            core,
        });
        sidecar.register_component(server.clone()).await?;
        Ok(server)
    }

    pub fn router() -> Router<AppState> {
        let api_v1_router = {
            let user_router = Router::new()
                .route(
                    "/register",
                    wrap_post_handler(user::register, ApiConfig::default().with_from_ipc()),
                )
                .route(
                    "/login",
                    wrap_get_handler(user::login, ApiConfig::default()),
                )
                .route(
                    "/refresh-token",
                    wrap_get_handler(user::refresh_token, ApiConfig::default().with_auth()),
                );

            Router::new().nest("/user", user_router)
        };

        Router::new()
            .route("/ping", wrap_get_handler(ping, ApiConfig::default()))
            .nest("/api/v1", api_v1_router)
    }

    pub async fn is_socket_in_use(&self) -> bool {
        let ipc_file_path = self.repo.ipc_file_path();

        if !ipc_file_path.exists() {
            return false;
        }

        match UnixStream::connect(ipc_file_path).await {
            Ok(stream) => {
                let _ = stream.as_raw_fd();
                true
            }
            Err(err) => {
                if err.kind() == io::ErrorKind::ConnectionRefused {
                    false
                } else {
                    warn!(err= ?err, "check socket connect failed");
                    false
                }
            }
        }
    }
}

#[async_trait]
impl Component for Server {
    fn name(&self) -> &str {
        &self.sidecar.current_component_name
    }

    async fn start(&self) -> Result<()> {
        let root_router = Self::router();

        let ipc_file_path = self.repo.ipc_file_path();
        if self.is_socket_in_use().await {
            bail!(
                "Ipc file is in use, may be other process is running: {}",
                ipc_file_path.display()
            );
        }

        if ipc_file_path.exists() {
            fs::remove_file(&ipc_file_path).await.wrap_err(format!(
                "Failed to remove ipc file: {}",
                ipc_file_path.display()
            ))?;
        }

        let listener = UnixListener::bind(ipc_file_path.clone()).wrap_err(format!(
            "Failed to bind ipc file, may be other process is running: {}",
            ipc_file_path.display()
        ))?;
        info!("ipc server listen on: {}", ipc_file_path.display());
        self.sidecar.spawn_core_task("ipc-listener", {
            let root_router = root_router.clone().with_state(AppState {
                core: self.core.clone(),
                is_ipc: true,
            });
            let sidecar = self.sidecar.clone();
            async move {
                axum::serve(listener, root_router)
                    .with_graceful_shutdown(async move {
                        if let Err(e) = sidecar.canceled().await {
                            warn!("ipc server cancel error: {}", e);
                        }
                    })
                    .await
            }
        });

        if self.repo.cfg.http.enable {
            let listener =
                TcpListener::bind(format!("0.0.0.0:{}", self.repo.cfg.http.port)).await?;
            info!(
                "http server listen on: http://127.0.0.1:{}",
                self.repo.cfg.http.port
            );
            self.sidecar.spawn_core_task("http-listener", {
                let mut root_router = root_router.clone().with_state(AppState {
                    core: self.core.clone(),
                    is_ipc: false,
                });
                let sidecar = self.sidecar.clone();
                let host = format!(
                    "{}:{}",
                    self.repo.cfg.http.swagger.host, self.repo.cfg.http.port
                );
                let swagger_enable = self.repo.cfg.http.swagger.enable;
                if swagger_enable {
                    info!("swagger ui listen on: {}/swagger-ui", host);
                }
                async move {
                    if swagger_enable {
                        let mut doc = base_openapi_doc();
                        doc.servers = Some(vec![
                            utoipa::openapi::ServerBuilder::new()
                                .url(host.clone())
                                .build(),
                        ]);
                        root_router = root_router.merge(
                            SwaggerUi::new("/swagger-ui").url("/swagger-ui/openapi.json", doc),
                        );
                    }

                    axum::serve(
                        listener,
                        root_router.into_make_service_with_connect_info::<SocketAddr>(),
                    )
                    .with_graceful_shutdown(async move {
                        if let Err(e) = sidecar.canceled().await {
                            warn!("http server cancel error: {}", e);
                        }
                    })
                    .await
                }
            });
        }

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let ipc_file_path = self.repo.ipc_file_path();
        if ipc_file_path.exists() {
            if let Err(e) = fs::remove_file(ipc_file_path).await {
                warn!("failed to remove ipc file: {}", e);
            }
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct PingReq {
    /// Ping content, optional
    #[param(example = "ping")]
    content: Option<String>,
}

#[utoipa::path(
    tag = "system",
    get,
    path = "/ping",
    params(PingReq),
    responses((status = 200, description = "Success", body = Response<String>))
)]
async fn ping(
    _state: Arc<Core>,
    ctx: Context,
    _headers: HeaderMap,
    req: PingReq,
) -> Result<String> {
    let content = req.content.unwrap_or("".to_string());
    ctx.add_log_field("content", content.clone()).await;
    Ok(content)
}

#[derive(Default, Debug, Clone)]
pub struct ApiConfig {
    need_auth: bool,
    need_from_ipc: bool,
}

impl ApiConfig {
    fn with_auth(mut self) -> Self {
        self.need_auth = true;
        self
    }

    fn with_from_ipc(mut self) -> Self {
        self.need_from_ipc = true;
        self
    }
}

async fn pre_check(
    state: &AppState,
    cfg: &ApiConfig,
    ctx: &mut Context,
    headers: &HeaderMap,
) -> Result<()> {
    if cfg.need_from_ipc && !state.is_ipc {
        return Err(Error::ApiMustRequestFromIPC.into());
    }

    if cfg.need_from_ipc || !cfg.need_auth {
        return Ok(());
    }

    let Some(authorization) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return Err(Error::Unauthorized.into());
    };

    let mut parts = authorization.split_whitespace();
    let Some(scheme) = parts.next() else {
        return Err(Error::Unauthorized.into());
    };
    let Some(token) = parts.next() else {
        return Err(Error::Unauthorized.into());
    };

    if !scheme.eq_ignore_ascii_case("bearer") {
        return Err(Error::Unauthorized.into());
    }

    let hmac_key = state.core.repo.cfg.http.jwt.token_hmac_key.clone();
    let (user_id, _) = jwt::parse_with_hmac_key::<Value>(&hmac_key, token)
        .map_err(|_| eyre!(Error::Unauthorized))?;
    ctx.user_id = user_id;

    Ok(())
}

async fn snapshot_log_fields(
    storage: &Arc<RwLock<Vec<(String, String)>>>,
) -> BTreeMap<String, String> {
    let guard = storage.read().await;
    guard.iter().cloned().collect()
}

fn restore_error_from_report(report: &Report) -> Error {
    report
        .downcast_ref::<Error>()
        .cloned()
        .or_else(|| {
            report
                .chain()
                .find_map(|cause| cause.downcast_ref::<Error>().cloned())
        })
        .unwrap_or_else(|| Error::Unknown(report.to_string()))
}

pub fn one_line_error(err: &Report) -> String {
    let mut out = String::new();
    for (i, e) in err.chain().enumerate() {
        if i > 0 {
            out.push_str(": ");
        }
        let mut msg = e.to_string();
        msg = msg.replace('\n', " ").replace('\r', " ");
        out.push_str(&msg);
    }
    out
}

fn extract_location_from_debug(err: &Report) -> Option<String> {
    let debug = format!("{:?}", err);
    let mut lines = debug.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "Location:" {
            if let Some(location_line) = lines.next() {
                let cleaned = strip_str(location_line);
                let trimmed = cleaned.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

async fn wrap_handler<Res, Fut, F>(
    state: AppState,
    cfg: ApiConfig,
    client_ip: String,
    method: &'static str,
    uri_path: String,
    headers: HeaderMap,
    fut_factory: F,
) -> AxumResponse
where
    Res: Serialize + Send + 'static,
    Fut: Future<Output = Result<Res>> + Send,
    F: FnOnce(Arc<Core>, Context, HeaderMap) -> Fut,
{
    let mut ctx = Context::default();
    let start = Instant::now();
    let result = {
        if let Err(err) = pre_check(&state, &cfg, &mut ctx, &headers).await {
            Err(err)
        } else {
            fut_factory(state.core, ctx.clone(), headers).await
        }
    };
    let elapsed = start.elapsed();

    match result {
        Ok(data) => {
            let log_fields = snapshot_log_fields(&ctx.log_fields).await;
            info!(
                user = ctx.user_id,
                method = method,
                uri = uri_path,
                client_ip = client_ip,
                log_fields = debug(&log_fields),
                elapsed = ?elapsed,
                "api request"
            );
            Response::ok(data).into_response()
        }
        Err(err) => {
            let code_err = restore_error_from_report(&err);

            let log_fields = snapshot_log_fields(&ctx.log_fields).await;
            let log_fields_on_error = snapshot_log_fields(&ctx.log_fields_on_error).await;

            warn!(
                user = ctx.user_id,
                method = method,
                uri = uri_path,
                err_code = code_err.code(),
                err = one_line_error(&err),
                err_location = extract_location_from_debug(&err),
                client_ip = client_ip,
                log_fields = debug(&log_fields),
                log_fields_on_error = debug(&log_fields_on_error),
                elapsed = ?elapsed,
                "api request failed"
            );

            Response::<Res> {
                code: code_err.code(),
                msg: one_line_error(&err).to_string(),
                data: None,
            }
            .into_response()
        }
    }
}

async fn handle_param_error<Res>(
    method: &'static str,
    uri_path: String,
    client_ip: String,
    rejection_msg: String,
) -> AxumResponse
where
    Res: Serialize + Send + 'static,
{
    let err = Error::InvidRequestParameter(rejection_msg);

    warn!(
        method = method,
        uri = uri_path,
        err_code = err.code(),
        err = ?err,
        client_ip = client_ip,
        elapsed = 0,
        "api request failed"
    );

    Response::<Res>::err(&err).into_response()
}

async fn handle_request<Req, Res, Rej, MapRejection, H, Fut>(
    state: AppState,
    cfg: ApiConfig,
    client_ip: String,
    method: &'static str,
    uri_path: String,
    headers: HeaderMap,
    request: Result<Req, Rej>,
    map_rejection: MapRejection,
    handler: H,
) -> AxumResponse
where
    Req: Send + 'static,
    Res: Serialize + Send + 'static,
    Rej: Send,
    MapRejection: FnOnce(Rej) -> String + Send,
    H: FnOnce(Arc<Core>, Context, HeaderMap, Req) -> Fut + Send,
    Fut: Future<Output = Result<Res>> + Send,
{
    match request {
        Ok(req) => {
            wrap_handler::<Res, _, _>(
                state,
                cfg,
                client_ip,
                method,
                uri_path,
                headers,
                |state, ctx, headers| handler(state, ctx, headers, req),
            )
            .await
        }
        Err(rejection) => {
            let message = map_rejection(rejection);
            handle_param_error::<Res>(method, uri_path, client_ip, message).await
        }
    }
}

pub fn wrap_get_handler<Q, Res, H, Fut>(handler: H, cfg: ApiConfig) -> MethodRouter<AppState>
where
    Q: DeserializeOwned + Send + 'static,
    Res: Serialize + Send + 'static,
    H: Clone + Send + Sync + 'static,
    H: Fn(Arc<Core>, Context, HeaderMap, Q) -> Fut,
    Fut: Future<Output = Result<Res>> + Send + 'static,
{
    get(
        move |State(state): State<AppState>,
              ClientIp(client_ip): ClientIp,
              OriginalUri(uri): OriginalUri,
              headers,
              query: Result<Query<Q>, QueryRejection>| {
            let handler = handler.clone();
            let uri_path = uri.path().to_string();
            let cfg = cfg.clone();
            async move {
                let client_ip = client_ip.to_string();
                handle_request(
                    state,
                    cfg,
                    client_ip,
                    "get",
                    uri_path,
                    headers,
                    query.map(|Query(query)| query),
                    |rejection| rejection.body_text(),
                    move |state, ctx, headers, query| handler(state, ctx, headers, query),
                )
                .await
            }
        },
    )
}

pub fn wrap_post_handler<Req, Res, H, Fut>(handler: H, cfg: ApiConfig) -> MethodRouter<AppState>
where
    Req: DeserializeOwned + Send + 'static,
    Res: Serialize + Send + 'static,
    H: Clone + Send + Sync + 'static,
    H: Fn(Arc<Core>, Context, HeaderMap, Req) -> Fut,
    Fut: Future<Output = Result<Res>> + Send + 'static,
{
    post(
        move |State(state): State<AppState>,
              ClientIp(client_ip): ClientIp,
              OriginalUri(uri): OriginalUri,
              headers,
              json: Result<Json<Req>, JsonRejection>| {
            let handler = handler.clone();
            let uri_path = uri.path().to_string();
            let cfg = cfg.clone();
            async move {
                let client_ip = client_ip.to_string();
                handle_request(
                    state,
                    cfg,
                    client_ip,
                    "post",
                    uri_path,
                    headers,
                    json.map(|Json(json)| json),
                    |rejection| rejection.body_text(),
                    move |state, ctx, headers, json| handler(state, ctx, headers, json),
                )
                .await
            }
        },
    )
}

pub struct ClientIp(pub IpAddr);

impl<S> FromRequestParts<S> for ClientIp
where
    S: Send + Sync,
{
    type Rejection = (axum::http::StatusCode, String);

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            if let Ok(RightmostXForwardedFor(ip)) =
                RightmostXForwardedFor::from_request_parts(parts, state).await
            {
                return Ok(ClientIp(ip));
            }

            if let Ok(RightmostForwarded(ip)) =
                RightmostForwarded::from_request_parts(parts, state).await
            {
                return Ok(ClientIp(ip));
            }

            if let Ok(TrueClientIp(ip)) = TrueClientIp::from_request_parts(parts, state).await {
                return Ok(ClientIp(ip));
            }

            if let Ok(CloudFrontViewerAddress(ip)) =
                CloudFrontViewerAddress::from_request_parts(parts, state).await
            {
                return Ok(ClientIp(ip));
            }

            if let Ok(FlyClientIp(ip)) = FlyClientIp::from_request_parts(parts, state).await {
                return Ok(ClientIp(ip));
            }

            if let Some(ConnectInfo(addr)) = parts.extensions.get::<ConnectInfo<SocketAddr>>() {
                return Ok(ClientIp(addr.ip()));
            }

            Ok(ClientIp(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use sidecar::prelude::{Report, WrapErr};

    use super::*;

    #[test]
    fn restore_error_returns_known_variant() {
        let report: Report = Error::ApiMustRequestFromIPC.into();
        let restored = restore_error_from_report(&report);
        assert!(matches!(restored, Error::ApiMustRequestFromIPC));
    }

    #[test]
    fn restore_error_falls_back_to_unknown() {
        let report: Report = io::Error::new(io::ErrorKind::Other, "boom").into();
        let restored = restore_error_from_report(&report);
        match restored {
            Error::Unknown(msg) => assert!(msg.contains("boom")),
            other => panic!("expected unknown fallback, got {other:?}"),
        }
    }

    #[test]
    fn restore_error_handles_nested_reports() {
        let report: Report = Error::ApiMustRequestFromIPC.into();
        let report = report.wrap_err("layer one").wrap_err("layer two");
        let restored = restore_error_from_report(&report);
        assert!(matches!(restored, Error::ApiMustRequestFromIPC));
    }

    #[test]
    fn extract_location_reads_location_block() {
        let _ = color_eyre::install();
        let result: std::result::Result<(), Error> = Err(Error::ApiMustRequestFromIPC);
        let report = result.wrap_err("write to ipc failed").unwrap_err();
        let location =
            extract_location_from_debug(&report).expect("should extract location from report");
        assert!(location.contains("server.rs"));
        assert!(
            !location.contains('\u{1b}'),
            "location still contains ansi escapes: {location}"
        );
    }
}
