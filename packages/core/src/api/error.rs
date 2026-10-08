//! The one error envelope every non-2xx core response carries:
//! `{"error": "<message>", "code": "<snake_case>"}` as JSON.

use crate::pairing::VerifyError;
use crate::pipeline::InputError;
use crate::sender::SenderError;
use crate::session::ActivateError;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

#[derive(Serialize)]
struct Envelope<'a> {
    error: &'a str,
    code: &'a str,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        ApiError {
            status,
            code,
            message: message.into(),
        }
    }

    /// 400: the request shape or values are invalid.
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }

    /// 401: no valid pairing token and not the loopback desktop.
    pub fn not_paired() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "not_paired",
            "pair this device with the desktop PIN first",
        )
    }

    /// 403: a loopback-only endpoint was called from another host.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    /// 404: unknown device, input or route.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    /// 404: the request needs a live output and there is none.
    pub fn no_active_output() -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "no_active_output",
            "no active output",
        )
    }

    /// 409: exclusivity or state conflict (previous output would not stop,
    /// no disc in the drive, device not ready).
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", message)
    }

    /// 429: too many wrong PINs from this peer.
    pub fn pin_lockout() -> Self {
        Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            "pin_lockout",
            "too many wrong PINs; wait before trying again",
        )
    }

    /// 502: the speaker, sidecar or helper could not be reached.
    pub fn transport_unreachable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, "transport_unreachable", message)
    }

    /// 503: the service is paused from the desktop.
    pub fn service_paused() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_paused",
            "the on-air service is paused",
        )
    }

    /// 500: a local failure that is not the client's fault.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", message)
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = axum::Json(Envelope {
            error: &self.message,
            code: self.code,
        });
        (self.status, body).into_response()
    }
}

impl From<SenderError> for ApiError {
    fn from(error: SenderError) -> Self {
        let message = error.to_string();
        match error.root() {
            SenderError::NoActiveOutput => ApiError::no_active_output(),
            SenderError::DeviceNotFound(_) => ApiError::not_found(message),
            SenderError::NotReady(_) => ApiError::conflict(message),
            SenderError::Transport(_) => match error {
                SenderError::StopFailed(_) => ApiError::conflict(message),
                _ => ApiError::transport_unreachable(message),
            },
            SenderError::Internal(_) => ApiError::internal(message),
            // `root()` never returns these wrappers.
            SenderError::StopFailed(_) | SenderError::RollbackFailed { .. } => {
                ApiError::conflict(message)
            }
        }
    }
}

impl From<ActivateError> for ApiError {
    fn from(error: ActivateError) -> Self {
        match error {
            ActivateError::NotFound => ApiError::not_found("output device not found"),
            ActivateError::UnknownTransport(_) | ActivateError::Unsupported(_) => {
                ApiError::validation(error.to_string())
            }
            ActivateError::NoLanAddress(_) | ActivateError::Discovery(_) => {
                ApiError::internal(error.to_string())
            }
            ActivateError::Sender(sender) => ApiError::from(sender),
        }
    }
}

impl From<InputError> for ApiError {
    fn from(error: InputError) -> Self {
        match error {
            InputError::NotFound(_) => ApiError::not_found(error.to_string()),
            InputError::NoDisc => ApiError::not_found(error.to_string()),
            InputError::Capture(_) => ApiError::internal(error.to_string()),
        }
    }
}

impl From<VerifyError> for ApiError {
    fn from(error: VerifyError) -> Self {
        match error {
            VerifyError::InvalidPin => ApiError::new(
                StatusCode::UNAUTHORIZED,
                "invalid_pin",
                "the PIN does not match the one shown on the desktop",
            ),
            VerifyError::RateLimited => ApiError::pin_lockout(),
            VerifyError::StorageUnavailable => {
                ApiError::internal("could not save the pairing on the desktop")
            }
        }
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        ApiError::validation(rejection.body_text())
    }
}

/// JSON request body whose rejection is the error envelope (400) instead of
/// axum's plain-text 400/415/422 replies.
pub struct JsonBody<T>(pub T);

#[async_trait::async_trait]
impl<T, S> FromRequest<S> for JsonBody<T>
where
    axum::Json<T>: FromRequest<S, Rejection = JsonRejection>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let axum::Json(value) = axum::Json::<T>::from_request(request, state).await?;
        Ok(JsonBody(value))
    }
}

/// Fallback for unknown routes so even a typo gets the envelope.
pub async fn route_not_found() -> ApiError {
    ApiError::not_found("no such route")
}

/// Fallback for a known route called with the wrong method.
pub async fn method_not_allowed() -> ApiError {
    ApiError::new(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "this route does not accept that method",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn envelope_is_json_with_error_and_code() {
        let response = ApiError::validation("bad").into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()["content-type"], "application/json");
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "bad");
        assert_eq!(json["code"], "invalid_request");
    }

    #[test]
    fn sender_errors_map_to_the_contract_statuses() {
        assert_eq!(
            ApiError::from(SenderError::NoActiveOutput).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ApiError::from(SenderError::transport("down")).status(),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            ApiError::from(SenderError::StopFailed(Box::new(SenderError::transport(
                "down"
            ))))
            .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            ApiError::from(SenderError::DeviceNotFound("x".into())).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ApiError::from(VerifyError::RateLimited).status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }
}
