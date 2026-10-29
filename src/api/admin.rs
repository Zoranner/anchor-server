use anchor_core::crypto;
use anchor_protocol::{DeviceStatus, HardwareInventory};
use axum::{
    extract::{rejection::JsonRejection, rejection::QueryRejection, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::{self, IssuedSession},
    domain::{device as device_domain, policy as policy_domain},
    entity::{admin_user, device, device_inventory_snapshot, signing_key},
    keys::StoredSigningKey,
    state::AppState,
};

use super::{device as device_api, error::ApiError, guard, pagination, pagination::Page};

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

#[derive(Debug, Serialize)]
pub struct RotateSigningKeyResponse {
    pub retired_key_id: String,
    pub signing_key: SigningKeyResponse,
}

#[derive(Debug, Deserialize)]
pub struct DeviceListQuery {
    pub status: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct InventorySummary {
    pub computer_model: String,
    pub os_name: String,
    pub os_version: String,
    pub cpu_model: String,
    pub memory_bytes: u64,
    pub disk_count: usize,
    pub network_adapter_count: usize,
}

#[derive(Debug, Serialize)]
pub struct AdminDeviceSummary {
    pub id: Uuid,
    pub installation_id: Uuid,
    pub hostname: String,
    pub status: DeviceStatus,
    pub client_version: Option<String>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub inventory_summary: Option<InventorySummary>,
}

#[derive(Debug, Serialize)]
pub struct AdminDeviceDetail {
    pub id: Uuid,
    pub installation_id: Uuid,
    pub hostname: String,
    pub status: DeviceStatus,
    pub client_version: Option<String>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub disabled_at: Option<DateTime<Utc>>,
    pub inventory: Option<HardwareInventory>,
    pub inventory_captured_at: Option<DateTime<Utc>>,
    pub inventory_received_at: Option<DateTime<Utc>>,
    pub effective_policy: Option<EffectivePolicyResponse>,
    pub last_policy_version: Option<u64>,
    pub last_policy_fetched_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct EffectivePolicyResponse {
    pub policy_id: Uuid,
    pub version: u64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct DeviceActionResponse {
    pub id: Uuid,
    pub status: DeviceStatus,
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
    let context = guard::require_admin(&state.db, &headers, true).await?;
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
    let context = guard::require_admin(&state.db, &headers, false).await?;
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

pub async fn rotate_signing_key(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<RotateSigningKeyResponse>, ApiError> {
    let context = guard::require_admin(&state.db, &headers, true).await?;
    let stored_key = prepare_signing_key(&state, None)?;
    match persist_rotation(&state, context.admin.id, &stored_key).await {
        Ok((retired, active)) => Ok(Json(RotateSigningKeyResponse {
            retired_key_id: retired.key_id,
            signing_key: active.into(),
        })),
        Err(RotationPersistFailure::SafeToRemove(error)) => {
            if stored_key.created_file {
                if let Err(remove_error) = state.signing_keys.remove_file(&stored_key.key_id) {
                    tracing::error!(
                        error = %remove_error,
                        key_id = %stored_key.key_id,
                        "failed to remove unused rotated signing key file"
                    );
                }
            }
            Err(error)
        }
        Err(RotationPersistFailure::CommitUncertain(error)) => {
            tracing::error!(
                key_id = %stored_key.key_id,
                "signing key rotation commit result is uncertain; keeping encrypted key file"
            );
            Err(error)
        }
    }
}

pub async fn list_devices(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: Result<Query<DeviceListQuery>, QueryRejection>,
) -> Result<Json<Page<AdminDeviceSummary>>, ApiError> {
    guard::require_admin(&state.db, &headers, false).await?;
    let Query(query) =
        query.map_err(|_| ApiError::bad_request("invalid device list query parameters"))?;
    let limit = pagination::limit(query.limit)?;
    let status = query
        .status
        .as_deref()
        .map(|value| {
            DeviceStatus::parse(value)
                .ok_or_else(|| ApiError::bad_request("unknown device status filter"))
        })
        .transpose()?;
    let cursor = query
        .cursor
        .as_deref()
        .map(pagination::parse_cursor)
        .transpose()?;

    let mut select = device::Entity::find();
    if let Some(status) = status {
        select = select.filter(device::Column::Status.eq(status.as_str()));
    }
    if let Some((created_at, id)) = cursor {
        select = select.filter(
            Condition::any()
                .add(device::Column::CreatedAt.lt(created_at))
                .add(
                    Condition::all()
                        .add(device::Column::CreatedAt.eq(created_at))
                        .add(device::Column::Id.lt(id)),
                ),
        );
    }

    let mut devices = select
        .order_by_desc(device::Column::CreatedAt)
        .order_by_desc(device::Column::Id)
        .limit(limit + 1)
        .all(&state.db)
        .await
        .map_err(ApiError::internal)?;
    let has_more = devices.len() > limit as usize;
    if has_more {
        devices.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        devices
            .last()
            .and_then(|device| pagination::encode_cursor(device.created_at, device.id))
    } else {
        None
    };

    let mut items = Vec::with_capacity(devices.len());
    for device in devices {
        let snapshot = latest_inventory_snapshot(&state, device.id).await?;
        let status = device_domain::status_of(&device).map_err(device_api::map_device_error)?;
        let inventory_summary = snapshot
            .as_ref()
            .map(inventory_from_snapshot)
            .transpose()?
            .map(|inventory| summarize_inventory(&inventory));
        items.push(AdminDeviceSummary {
            id: device.id,
            installation_id: device.installation_id,
            hostname: device.hostname,
            status,
            client_version: device.client_version,
            last_seen_at: device.last_seen_at,
            created_at: device.created_at,
            approved_at: device.approved_at,
            inventory_summary,
        });
    }

    Ok(Json(Page { items, next_cursor }))
}

pub async fn device_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(device_id): Path<String>,
) -> Result<Json<AdminDeviceDetail>, ApiError> {
    guard::require_admin(&state.db, &headers, false).await?;
    let device_id = parse_device_id(&device_id)?;
    let device = device::Entity::find_by_id(device_id)
        .one(&state.db)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(device_not_found)?;
    let snapshot = latest_inventory_snapshot(&state, device.id).await?;
    let inventory = snapshot.as_ref().map(inventory_from_snapshot).transpose()?;
    let inventory_captured_at = snapshot.as_ref().map(|value| value.captured_at);
    let inventory_received_at = snapshot.as_ref().map(|value| value.created_at);
    let status = device_domain::status_of(&device).map_err(device_api::map_device_error)?;
    let effective_policy = match policy_domain::policy_for_device(&state.db, device.id).await {
        Ok(Some(policy)) => policy.version.and_then(|version| {
            u64::try_from(version)
                .ok()
                .map(|version| EffectivePolicyResponse {
                    policy_id: policy.id,
                    version,
                    expires_at: policy.expires_at,
                })
        }),
        Ok(None) => None,
        Err(error) => return Err(ApiError::internal(error)),
    };

    Ok(Json(AdminDeviceDetail {
        id: device.id,
        installation_id: device.installation_id,
        hostname: device.hostname,
        status,
        client_version: device.client_version,
        last_seen_at: device.last_seen_at,
        created_at: device.created_at,
        approved_at: device.approved_at,
        disabled_at: device.disabled_at,
        inventory,
        inventory_captured_at,
        inventory_received_at,
        effective_policy,
        last_policy_version: device
            .last_policy_version
            .and_then(|version| u64::try_from(version).ok()),
        last_policy_fetched_at: device.last_policy_fetched_at,
    }))
}

pub async fn approve_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(device_id): Path<String>,
) -> Result<Json<DeviceActionResponse>, ApiError> {
    let (context, device_id) = admin_device_action(&state, &headers, &device_id).await?;
    device_domain::approve_device(&state.db, device_id, context.admin.id)
        .await
        .map_err(device_api::map_device_error)?;
    Ok(Json(DeviceActionResponse {
        id: device_id,
        status: DeviceStatus::Active,
    }))
}

pub async fn reject_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(device_id): Path<String>,
) -> Result<Json<DeviceActionResponse>, ApiError> {
    let (_, device_id) = admin_device_action(&state, &headers, &device_id).await?;
    device_domain::reject_device(&state.db, device_id)
        .await
        .map_err(device_api::map_device_error)?;
    Ok(Json(DeviceActionResponse {
        id: device_id,
        status: DeviceStatus::Rejected,
    }))
}

pub async fn disable_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(device_id): Path<String>,
) -> Result<Json<DeviceActionResponse>, ApiError> {
    let (_, device_id) = admin_device_action(&state, &headers, &device_id).await?;
    device_domain::disable_device(&state.db, device_id)
        .await
        .map_err(device_api::map_device_error)?;
    Ok(Json(DeviceActionResponse {
        id: device_id,
        status: DeviceStatus::Disabled,
    }))
}

pub async fn reset_device_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(device_id): Path<String>,
) -> Result<Json<DeviceActionResponse>, ApiError> {
    let (_, device_id) = admin_device_action(&state, &headers, &device_id).await?;
    device_domain::reset_device_token(&state.db, device_id)
        .await
        .map_err(device_api::map_device_error)?;
    Ok(Json(DeviceActionResponse {
        id: device_id,
        status: DeviceStatus::Pending,
    }))
}

async fn admin_device_action(
    state: &AppState,
    headers: &HeaderMap,
    device_id: &str,
) -> Result<(crate::auth::AuthContext, Uuid), ApiError> {
    let context = guard::require_admin(&state.db, headers, true).await?;
    let device_id = parse_device_id(device_id)?;
    Ok((context, device_id))
}

fn parse_device_id(value: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(value).map_err(|_| ApiError::bad_request("device_id must be a UUID"))
}

fn device_not_found() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "device_not_found",
        "device was not found",
    )
}

async fn latest_inventory_snapshot(
    state: &AppState,
    device_id: Uuid,
) -> Result<Option<device_inventory_snapshot::Model>, ApiError> {
    device_inventory_snapshot::Entity::find()
        .filter(device_inventory_snapshot::Column::DeviceId.eq(device_id))
        .order_by_desc(device_inventory_snapshot::Column::CreatedAt)
        .order_by_desc(device_inventory_snapshot::Column::Id)
        .one(&state.db)
        .await
        .map_err(ApiError::internal)
}

fn inventory_from_snapshot(
    snapshot: &device_inventory_snapshot::Model,
) -> Result<HardwareInventory, ApiError> {
    serde_json::from_value(snapshot.inventory_json.clone())
        .map_err(|error| ApiError::internal(format!("decode hardware inventory: {error}")))
}

fn summarize_inventory(inventory: &HardwareInventory) -> InventorySummary {
    InventorySummary {
        computer_model: inventory.computer.model.clone(),
        os_name: inventory.operating_system.name.clone(),
        os_version: inventory.operating_system.version.clone(),
        cpu_model: inventory.cpu.model.clone(),
        memory_bytes: inventory
            .memory_modules
            .iter()
            .fold(0_u64, |total, module| {
                total.saturating_add(module.capacity_bytes)
            }),
        disk_count: inventory.disks.len(),
        network_adapter_count: inventory.network_adapters.len(),
    }
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

enum RotationPersistFailure {
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

async fn persist_rotation(
    state: &AppState,
    admin_id: Uuid,
    stored_key: &StoredSigningKey,
) -> Result<(signing_key::Model, signing_key::Model), RotationPersistFailure> {
    let tx = state
        .db
        .begin()
        .await
        .map_err(|error| RotationPersistFailure::SafeToRemove(ApiError::internal(error)))?;
    if let Err(error) = tx
        .execute_unprepared(&format!(
            "SELECT pg_advisory_xact_lock({SETUP_ADVISORY_LOCK_ID})"
        ))
        .await
    {
        let _ = tx.rollback().await;
        return Err(RotationPersistFailure::SafeToRemove(ApiError::internal(
            error,
        )));
    }

    let active = match signing_key::Entity::find()
        .filter(signing_key::Column::Status.eq("active"))
        .lock_exclusive()
        .one(&tx)
        .await
    {
        Ok(Some(active)) => active,
        Ok(None) => {
            let _ = tx.rollback().await;
            return Err(RotationPersistFailure::SafeToRemove(ApiError::conflict(
                "signing_key_unavailable",
                "no active signing key is available",
            )));
        }
        Err(error) => {
            let _ = tx.rollback().await;
            return Err(RotationPersistFailure::SafeToRemove(ApiError::internal(
                error,
            )));
        }
    };

    let now = chrono::Utc::now();
    let mut retired_active: signing_key::ActiveModel = active.into();
    retired_active.status = Set("retired".into());
    retired_active.retired_at = Set(Some(now));
    let retired = match retired_active.update(&tx).await {
        Ok(retired) => retired,
        Err(error) => {
            let _ = tx.rollback().await;
            return Err(RotationPersistFailure::SafeToRemove(ApiError::internal(
                error,
            )));
        }
    };

    let new_key = signing_key::ActiveModel {
        key_id: Set(stored_key.key_id.clone()),
        public_key_hex: Set(stored_key.public_key_hex.clone()),
        status: Set("active".into()),
        created_by_admin_id: Set(admin_id),
        created_at: Set(now),
        activated_at: Set(now),
        retired_at: Set(None),
        revoked_at: Set(None),
    };
    let active = match new_key.insert(&tx).await {
        Ok(active) => active,
        Err(error) => {
            let _ = tx.rollback().await;
            return Err(RotationPersistFailure::SafeToRemove(ApiError::internal(
                error,
            )));
        }
    };

    tx.commit()
        .await
        .map_err(|error| RotationPersistFailure::CommitUncertain(ApiError::internal(error)))?;
    Ok((retired, active))
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
