use anchor_core::{BurningRules, NetworkRules, PolicyPackage, PolicyPayload, ScanRules, UsbRules};
use anchor_protocol::ServerKeyringResponse;
use axum::{
    extract::{
        rejection::{JsonRejection, QueryRejection},
        Path, Query, State,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    domain::{
        device as device_domain,
        policy::{
            self as policy_domain, AssignmentTarget, PolicyDraftInput, PolicyError, PolicyStatus,
        },
    },
    entity::policy,
    state::AppState,
};

use super::{device as device_api, error::ApiError, guard, pagination, pagination::Page};

#[derive(Debug, Deserialize)]
pub struct CreatePolicyRequest {
    pub expires_at: DateTime<Utc>,
    pub usb: UsbRules,
    pub network: NetworkRules,
    pub burning: BurningRules,
    pub scan: ScanRules,
}

#[derive(Debug, Deserialize)]
pub struct AssignPolicyRequest {
    #[serde(default)]
    pub device_id: Option<Uuid>,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Deserialize)]
pub struct PolicyListQuery {
    pub status: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct PolicySummary {
    pub id: Uuid,
    pub version: Option<u64>,
    pub status: PolicyStatus,
    pub expires_at: DateTime<Utc>,
    pub created_by_admin_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub signed_at: Option<DateTime<Utc>>,
    pub server_key_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PolicyDetail {
    #[serde(flatten)]
    pub summary: PolicySummary,
    pub policy_json: serde_json::Value,
    pub signature_hex: Option<String>,
}

pub async fn create_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<CreatePolicyRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<PolicyDetail>), ApiError> {
    let Json(request) = payload.map_err(ApiError::from)?;
    let context = guard::require_admin(&state.db, &headers, true).await?;
    let policy = policy_domain::create_draft(
        &state.db,
        context.admin.id,
        PolicyDraftInput {
            expires_at: request.expires_at,
            usb: request.usb,
            network: request.network,
            burning: request.burning,
            scan: request.scan,
        },
    )
    .await
    .map_err(map_policy_error)?;
    Ok((StatusCode::CREATED, Json(policy_detail(&policy)?)))
}

pub async fn list_policies(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: Result<Query<PolicyListQuery>, QueryRejection>,
) -> Result<Json<Page<PolicySummary>>, ApiError> {
    guard::require_admin(&state.db, &headers, false).await?;
    let Query(query) =
        query.map_err(|_| ApiError::bad_request("invalid policy list query parameters"))?;
    let limit = pagination::limit(query.limit)?;
    let status = query
        .status
        .as_deref()
        .map(|value| {
            PolicyStatus::parse(value)
                .ok_or_else(|| ApiError::bad_request("unknown policy status filter"))
        })
        .transpose()?;
    let cursor = query
        .cursor
        .as_deref()
        .map(pagination::parse_cursor)
        .transpose()?;

    let mut select = policy::Entity::find();
    if let Some(status) = status {
        select = select.filter(policy::Column::Status.eq(status.as_str()));
    }
    if let Some((created_at, id)) = cursor {
        select = select.filter(
            Condition::any()
                .add(policy::Column::CreatedAt.lt(created_at))
                .add(
                    Condition::all()
                        .add(policy::Column::CreatedAt.eq(created_at))
                        .add(policy::Column::Id.lt(id)),
                ),
        );
    }
    let mut policies = select
        .order_by_desc(policy::Column::CreatedAt)
        .order_by_desc(policy::Column::Id)
        .limit(limit + 1)
        .all(&state.db)
        .await
        .map_err(ApiError::internal)?;
    let has_more = policies.len() > limit as usize;
    if has_more {
        policies.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        policies
            .last()
            .and_then(|policy| pagination::encode_cursor(policy.created_at, policy.id))
    } else {
        None
    };
    let mut items = Vec::with_capacity(policies.len());
    for policy in policies {
        items.push(policy_summary(&policy)?);
    }
    Ok(Json(Page { items, next_cursor }))
}

pub async fn policy_detail_endpoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(policy_id): Path<String>,
) -> Result<Json<PolicyDetail>, ApiError> {
    guard::require_admin(&state.db, &headers, false).await?;
    let policy = load_policy(&state, &policy_id).await?;
    Ok(Json(policy_detail(&policy)?))
}

pub async fn sign_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(policy_id): Path<String>,
) -> Result<Json<PolicyDetail>, ApiError> {
    guard::require_admin(&state.db, &headers, true).await?;
    let policy_id = parse_policy_id(&policy_id)?;
    let policy = policy_domain::sign_policy(&state.db, &state.signing_keys, policy_id)
        .await
        .map_err(map_policy_error)?;
    Ok(Json(policy_detail(&policy)?))
}

pub async fn assign_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(policy_id): Path<String>,
    payload: Result<Json<AssignPolicyRequest>, JsonRejection>,
) -> Result<StatusCode, ApiError> {
    let Json(request) = payload.map_err(ApiError::from)?;
    let context = guard::require_admin(&state.db, &headers, true).await?;
    let policy_id = parse_policy_id(&policy_id)?;
    let target = match (request.device_id, request.is_default) {
        (Some(device_id), false) => AssignmentTarget::Device(device_id),
        (None, true) => AssignmentTarget::Default,
        _ => {
            return Err(ApiError::bad_request(
                "exactly one of device_id or is_default must be set",
            ));
        }
    };
    policy_domain::assign_policy(&state.db, context.admin.id, policy_id, target)
        .await
        .map_err(map_policy_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn export_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(policy_id): Path<String>,
) -> Result<Response, ApiError> {
    guard::require_admin(&state.db, &headers, false).await?;
    let policy = load_policy(&state, &policy_id).await?;
    if policy_domain::policy_status(&policy).map_err(map_policy_error)? != PolicyStatus::Signed {
        return Err(map_policy_error(PolicyError::NotSigned));
    }
    let envelope = policy_domain::envelope(&policy).map_err(map_policy_error)?;
    let payload: PolicyPayload = serde_json::from_value(envelope.policy_json)
        .map_err(|error| ApiError::internal(format!("decode policy export: {error}")))?;
    let package = PolicyPackage {
        payload,
        signature: envelope.signature_hex,
    };
    let mut response = Json(package).into_response();
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"anchor-guard-policy-v{}.json\"",
            envelope.version
        ))
        .map_err(ApiError::internal)?,
    );
    Ok(response)
}

pub async fn device_keyring(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ServerKeyringResponse>, ApiError> {
    let device = device_api::authenticate_device(&state, &headers).await?;
    device_domain::ensure_active(&device).map_err(device_api::map_device_error)?;
    let keys = policy_domain::keyring(&state.db)
        .await
        .map_err(map_policy_error)?;
    Ok(Json(ServerKeyringResponse { keys }))
}

pub async fn device_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let device = device_api::authenticate_device(&state, &headers).await?;
    device_domain::ensure_active(&device).map_err(device_api::map_device_error)?;
    let policy = policy_domain::policy_for_device(&state.db, device.id)
        .await
        .map_err(map_policy_error)?;
    match policy {
        Some(policy) => {
            let envelope = policy_domain::envelope(&policy).map_err(map_policy_error)?;
            Ok(Json(envelope).into_response())
        }
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}

fn policy_summary(policy: &policy::Model) -> Result<PolicySummary, ApiError> {
    Ok(PolicySummary {
        id: policy.id,
        version: policy.version.map(|version| version as u64),
        status: policy_domain::policy_status(policy).map_err(map_policy_error)?,
        expires_at: policy.expires_at,
        created_by_admin_id: policy.created_by_admin_id,
        created_at: policy.created_at,
        signed_at: policy.signed_at,
        server_key_id: policy.server_key_id.clone(),
    })
}

fn policy_detail(policy: &policy::Model) -> Result<PolicyDetail, ApiError> {
    Ok(PolicyDetail {
        summary: policy_summary(policy)?,
        policy_json: policy.policy_json.clone(),
        signature_hex: policy.signature_hex.clone(),
    })
}

async fn load_policy(state: &AppState, policy_id: &str) -> Result<policy::Model, ApiError> {
    let policy_id = parse_policy_id(policy_id)?;
    policy::Entity::find_by_id(policy_id)
        .one(&state.db)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(policy_not_found)
}

fn parse_policy_id(value: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(value).map_err(|_| ApiError::bad_request("policy_id must be a UUID"))
}

fn policy_not_found() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "policy_not_found",
        "policy was not found",
    )
}

fn map_policy_error(error: PolicyError) -> ApiError {
    match error {
        PolicyError::InvalidRequest(message) => ApiError::bad_request(message),
        PolicyError::NotFound => policy_not_found(),
        PolicyError::NotDraft => {
            ApiError::conflict("policy_not_draft", "only a draft policy can be signed")
        }
        PolicyError::NotSigned => {
            ApiError::conflict("policy_not_signed", "policy must be signed first")
        }
        PolicyError::Expired => ApiError::conflict("policy_expired", "policy has already expired"),
        PolicyError::DeviceNotFound => ApiError::new(
            StatusCode::NOT_FOUND,
            "device_not_found",
            "device was not found",
        ),
        PolicyError::DeviceNotActive => ApiError::conflict(
            "device_not_active",
            "policy can only be assigned to an active device",
        ),
        PolicyError::SigningKeyUnavailable => ApiError::service_unavailable(
            "signing_key_unavailable",
            "active signing key is unavailable",
        ),
        PolicyError::Internal(error) => ApiError::internal(error),
        PolicyError::Database(error) => ApiError::internal(error),
    }
}
