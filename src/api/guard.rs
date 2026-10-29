use axum::{http::HeaderMap, http::StatusCode};
use sea_orm::DatabaseConnection;

use crate::auth::{self, AuthContext, AuthError};

use super::error::ApiError;

pub async fn require_admin(
    db: &DatabaseConnection,
    headers: &HeaderMap,
    require_csrf: bool,
) -> Result<AuthContext, ApiError> {
    let context = auth::authenticate(db, headers)
        .await
        .map_err(map_auth_error)?;
    if require_csrf {
        auth::validate_csrf(headers, &context.session).map_err(map_auth_error)?;
    }
    Ok(context)
}

fn map_auth_error(error: AuthError) -> ApiError {
    match error {
        AuthError::MissingSession
        | AuthError::InvalidSession
        | AuthError::ExpiredSession
        | AuthError::DisabledAdmin => ApiError::unauthorized("authentication required"),
        AuthError::MissingCsrf | AuthError::InvalidCsrf => ApiError::new(
            StatusCode::FORBIDDEN,
            "csrf_invalid",
            "CSRF token is invalid",
        ),
        AuthError::Database(error) => ApiError::internal(error),
    }
}
