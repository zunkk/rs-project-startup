use axum::Json;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::kit::error::Error;

/// Unified error response exposed externally
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(bound = "T: ToSchema")]
pub struct Response<T> {
    pub code: u64,
    pub msg: String,
    #[schema(nullable = true)]
    pub data: Option<T>,
}

impl<T> IntoResponse for Response<T>
where
    T: Serialize,
{
    fn into_response(self) -> axum::response::Response {
        let body = Json(self);
        (axum::http::StatusCode::OK, body).into_response()
    }
}

impl<T> Response<T>
where
    T: Serialize,
{
    pub fn ok(data: T) -> Self {
        Self {
            code: 0,
            msg: "".to_string(),
            data: Some(data),
        }
    }

    pub fn err(err: &Error) -> Self {
        Self {
            code: err.code(),
            msg: err.to_string(),
            data: None,
        }
    }
}
