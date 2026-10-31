//! API errors and the single JSON error shape (APP.md invariant 13).

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Every error the API returns on purpose. The message is safe to show to anyone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    BadRequest(&'static str),
    InvalidPin,
    WrongPin,
    Unavailable,
    NotFound,
    PayloadTooLarge,
    UnsupportedMediaType,
    RateLimited,
    ServiceUnavailable(&'static str),
    Internal,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
}

impl ApiError {
    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) | Self::InvalidPin => StatusCode::BAD_REQUEST,
            Self::WrongPin => StatusCode::FORBIDDEN,
            Self::Unavailable | Self::NotFound => StatusCode::NOT_FOUND,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    #[must_use]
    pub fn message(&self) -> &'static str {
        match self {
            Self::BadRequest(message) | Self::ServiceUnavailable(message) => message,
            Self::InvalidPin => "Invalid PIN format",
            Self::WrongPin => "Unable to retrieve message",
            Self::Unavailable => "Message unavailable",
            Self::NotFound => "Not found",
            Self::PayloadTooLarge => "Payload too large",
            Self::UnsupportedMediaType => "Unsupported media type",
            Self::RateLimited => "Rate limit exceeded",
            Self::Internal => "Internal error",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        error_response(self.status(), self.message())
    }
}

/// `{"error": message}` with the given status.
#[must_use]
pub fn error_response(status: StatusCode, message: &str) -> Response {
    (status, Json(ErrorBody { error: message })).into_response()
}

/// The message for a framework-generated error status, used by the JSON safety net.
#[must_use]
pub fn message_for_status(status: StatusCode) -> &'static str {
    match status {
        StatusCode::NOT_FOUND => "Not found",
        StatusCode::METHOD_NOT_ALLOWED => "Method not allowed",
        StatusCode::PAYLOAD_TOO_LARGE => "Payload too large",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "Unsupported media type",
        StatusCode::BAD_REQUEST => "Bad request",
        StatusCode::TOO_MANY_REQUESTS => "Rate limit exceeded",
        StatusCode::SERVICE_UNAVAILABLE => "Service unavailable",
        s if s.is_client_error() => "Request refused",
        _ => "Internal error",
    }
}
