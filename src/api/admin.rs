use anchor_core::crypto;
use axum::{
    extract::{rejection::JsonRejection, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::{self, AuthError, IssuedSession},
    entity::{admin_user, signing_key},
    keys::StoredSigningKey,
    state::AppState,
};

use super::error::ApiError;

const SETUP_ADVISORY_LOCK_ID: i64 = 836_420_731_892;

#[derive(Deserialize)]
pub struct SetupRequest {
    pub setup_token: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub signing_key_hex: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AdminResponse {
    pub id: Uuid,
    pub username: String,
    pub role: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<admin_user::Model> for AdminResponse {
    fn from(admin: admin_user::Model) -> Self {
        Self {
            id: admin.id,
            username: admin.username,
            role: admin.role,
            created_at: admin.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SigningKeyResponse {
    pub key_id: String,
    pub public_key_hex: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub activated_at: chrono::DateTime<chrono::Utc>,
}

impl From<signing_key::Model> for SigningKeyResponse {
    fn from(key: signing_key::Model) -> Self {
        Self {
            key_id: key.key_id,
            public_key_hex: key.public_key_hex,
            created_at: key.created_at,
            activated_at: key.activated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AdminSessionResponse {
    pub admin: AdminResponse,
    pub signing_key: Option<SigningKeyResponse>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct AdminMeResponse {
    pub admin: AdminResponse,
    pub signing_key: Option<SigningKeyResponse>,
}

pub async fn setup(
    State(state): State<AppState>,
    payload: Result<Json<SetupRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(request) = payload.map_err(ApiError::from)?;
    if admin_user::Entity::find()
        .count(&state.db)
        .await
        .map_err(ApiError::internal)?
        > 0
    {
        return Err(ApiError::conflict(
            "setup_complete",
            "initial setup has already been completed",
        ));
    }

    let expected_token = state.config.setup_token.as_deref().ok_or_else(|| {
        ApiError::service_unavailable("setup_unavailable", "ANCHOR_SETUP_TOKEN is not configured")
    })?;
    if request.setup_token.len() > 1024
        || !auth::secrets_equal(expected_token, &request.setup_token)
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "setup_token_invalid",
            "setup token is invalid",
        ));
    }

    let username = request.username.trim().to_owned();
    auth::validate_username(&username).map_err(ApiError::bad_request)?;
    auth::validate_password(&request.password).map_err(ApiError::bad_request)?;

    let password_hash = auth::hash_password(&request.password).map_err(ApiError::internal)?;
    let stored_key = prepare_signing_key(&state, request.signing_key_hex.as_deref())?;

    let (admin, key) = persist_setup(&state, &username, &password_hash, &stored_key).await?;
    let session = auth::create_session(&state.db, admin.id)
        .await
        .map_err(ApiError::internal)?;

    Ok(session_response(
        StatusCode::CREATED,
        AdminSessionResponse {
            admin: admin.into(),
            signing_key: Some(key.into()),
            expires_at: session.expires_at,
        },
        &session,
    ))
}

pub async fn login(
    State(state): State<AppState>,
    payload: Result<Json<LoginRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(request) = payload.map_err(ApiError::from)?;
    let username = request.username.trim();
    if username.is_empty()
        || username.len() > 64
        || request.password.is_empty()
        || request.password.len() > 1024
    {
        return Err(invalid_credentials());
    }

    let admin = admin_user::Entity::find()
        .filter(admin_user::Column::Username.eq(username))
        .one(&state.db)
        .await
        .map_err(ApiError::internal)?;

    let Some(admin) = admin else {
        auth::verify_dummy_password(&request.password);
        return Err(invalid_credentials());
    };
    if !auth::verify_password(&admin.password_hash, &request.password)
        || admin.disabled_at.is_some()
    {
        return Err(invalid_credentials());
    }

    let session = auth::create_session(&state.db, admin.id)
        .await
        .map_err(ApiError::internal)?;
    let key = current_signing_key(&state).await?;

    Ok(session_response(
        StatusCode::OK,
        AdminSessionResponse {
            admin: admin.into(),
            signing_key: key.map(Into::into),
            expires_at: session.expires_at,
        },
        &session,
    ))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let context = auth::authenticate(&state.db, &headers)
        .await
        .map_err(map_auth_error)?;
    auth::validate_csrf(&headers, &context.session).map_err(map_auth_error)?;
    auth::revoke_session(&state.db, &context.session)
        .await
        .map_err(ApiError::internal)?;

    let mut response = StatusCode::NO_CONTENT.into_response();
    for cookie in auth::clear_cookies() {
        response.headers_mut().append(header::SET_COOKIE, cookie);
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

pub async fn me(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    let context = auth::authenticate(&state.db, &headers)
        .await
        .map_err(map_auth_error)?;
    let key = current_signing_key(&state).await?;
    let mut response = Json(AdminMeResponse {
        admin: context.admin.into(),
        signing_key: key.map(Into::into),
    })
    .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn prepare_signing_key(
    state: &AppState,
    imported_signing_key_hex: Option<&str>,
) -> Result<StoredSigningKey, ApiError> {
    let signing_key = match imported_signing_key_hex
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(encoded) => crypto::signing_key_from_hex(encoded)
            .map_err(|_| ApiError::bad_request("signing_key_hex must be a 32-byte hex key"))?,
        None => crypto::generate_keypair().0,
    };
    let stored = state
        .signing_keys
        .store(&signing_key)
        .map_err(ApiError::internal)?;
    Ok(stored)
}

async fn persist_setup(
    state: &AppState,
    username: &str,
    password_hash: &str,
    stored_key: &StoredSigningKey,
) -> Result<(admin_user::Model, signing_key::Model), ApiError> {
    match persist_setup_inner(state, username, password_hash, stored_key).await {
        Ok(value) => Ok(value),
        Err(SetupPersistFailure::SafeToRemove(error)) => {
            if stored_key.created_file {
                if let Err(remove_error) = state.signing_keys.remove_file(&stored_key.key_id) {
                    tracing::error!(
                        error = %remove_error,
                        key_id = %stored_key.key_id,
                        "failed to remove unused signing key file"
                    );
                }
            }
            Err(error)
        }
        Err(SetupPersistFailure::CommitUncertain(error)) => {
            tracing::error!(
                key_id = %stored_key.key_id,
                "setup commit result is uncertain; keeping the encrypted signing key file"
            );
            Err(error)
        }
    }
}

enum SetupPersistFailure {
    SafeToRemove(ApiError),
    CommitUncertain(ApiError),
}

async fn persist_setup_inner(
    state: &AppState,
    username: &str,
    password_hash: &str,
    stored_key: &StoredSigningKey,
) -> Result<(admin_user::Model, signing_key::Model), SetupPersistFailure> {
    let tx = state
        .db
        .begin()
        .await
        .map_err(|error| SetupPersistFailure::SafeToRemove(ApiError::internal(error)))?;
    if let Err(error) = tx
        .execute_unprepared(&format!(
            "SELECT pg_advisory_xact_lock({SETUP_ADVISORY_LOCK_ID})"
        ))
        .await
    {
        let _ = tx.rollback().await;
        return Err(SetupPersistFailure::SafeToRemove(ApiError::internal(error)));
    }

    match admin_user::Entity::find().count(&tx).await {
        Ok(0) => {}
        Ok(_) => {
            let _ = tx.rollback().await;
            return Err(SetupPersistFailure::SafeToRemove(ApiError::conflict(
                "setup_complete",
                "initial setup has already been completed",
            )));
        }
        Err(error) => {
            let _ = tx.rollback().await;
            return Err(SetupPersistFailure::SafeToRemove(ApiError::internal(error)));
        }
    }

    let now = chrono::Utc::now();
    let admin_model = admin_user::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(username.into()),
        password_hash: Set(password_hash.into()),
        role: Set("admin".into()),
        created_at: Set(now),
        updated_at: Set(now),
        disabled_at: Set(None),
    };
    let admin = match admin_model.insert(&tx).await {
        Ok(admin) => admin,
        Err(error) => {
            let _ = tx.rollback().await;
            return Err(SetupPersistFailure::SafeToRemove(ApiError::internal(error)));
        }
    };

    let key_model = signing_key::ActiveModel {
        key_id: Set(stored_key.key_id.clone()),
        public_key_hex: Set(stored_key.public_key_hex.clone()),
        status: Set("active".into()),
        created_by_admin_id: Set(admin.id),
        created_at: Set(now),
        activated_at: Set(now),
        retired_at: Set(None),
        revoked_at: Set(None),
    };
    let key = match key_model.insert(&tx).await {
        Ok(key) => key,
        Err(error) => {
            let _ = tx.rollback().await;
            return Err(SetupPersistFailure::SafeToRemove(ApiError::internal(error)));
        }
    };

    tx.commit()
        .await
        .map_err(|error| SetupPersistFailure::CommitUncertain(ApiError::internal(error)))?;
    Ok((admin, key))
}

async fn current_signing_key(state: &AppState) -> Result<Option<signing_key::Model>, ApiError> {
    signing_key::Entity::find()
        .filter(signing_key::Column::Status.eq("active"))
        .order_by_desc(signing_key::Column::CreatedAt)
        .one(&state.db)
        .await
        .map_err(ApiError::internal)
}

fn session_response<T: Serialize>(
    status: StatusCode,
    body: T,
    session: &IssuedSession,
) -> Response {
    let mut response = (status, Json(body)).into_response();
    response
        .headers_mut()
        .append(header::SET_COOKIE, auth::session_cookie(&session.token));
    response
        .headers_mut()
        .append(header::SET_COOKIE, auth::csrf_cookie(&session.csrf_token));
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn invalid_credentials() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "invalid_credentials",
        "invalid username or password",
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_active_signing_key_status_is_serializable_through_response() {
        let now = chrono::Utc::now();
        let key = signing_key::Model {
            key_id: "a".repeat(64),
            public_key_hex: "b".repeat(64),
            status: "active".into(),
            created_by_admin_id: Uuid::new_v4(),
            created_at: now,
            activated_at: now,
            retired_at: None,
            revoked_at: None,
        };
        let response: SigningKeyResponse = key.into();
        assert_eq!(response.key_id.len(), 64);
        assert_eq!(response.public_key_hex.len(), 64);
    }
}
