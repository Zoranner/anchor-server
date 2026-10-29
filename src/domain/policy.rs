use anchor_core::{BurningRules, NetworkRules, PolicyPackage, PolicyPayload, ScanRules, UsbRules};
use anchor_protocol::{PolicyEnvelope, ServerKey, ServerKeyStatus};
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set, Statement, TransactionTrait,
};
use sea_query::Expr;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    entity::{device, policy, policy_assignment, signing_key},
    keys::SigningKeyStore,
};

use super::device::{status_of, DeviceError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyStatus {
    Draft,
    Signed,
}

impl PolicyStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Signed => "signed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "draft" => Some(Self::Draft),
            "signed" => Some(Self::Signed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PolicyDraftInput {
    pub expires_at: DateTime<Utc>,
    pub usb: UsbRules,
    pub network: NetworkRules,
    pub burning: BurningRules,
    pub scan: ScanRules,
}

impl PolicyDraftInput {
    pub fn into_payload(self, version: u64) -> Result<PolicyPayload, PolicyError> {
        let payload = PolicyPayload {
            version,
            expires_at: self.expires_at,
            usb: self.usb,
            network: self.network,
            burning: self.burning,
            scan: self.scan,
        };
        payload
            .validate()
            .map_err(|error| PolicyError::InvalidRequest(error.to_string()))?;
        Ok(payload)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentTarget {
    Device(Uuid),
    Default,
}

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error("policy not found")]
    NotFound,
    #[error("policy is not a draft")]
    NotDraft,
    #[error("policy is not signed")]
    NotSigned,
    #[error("policy has expired")]
    Expired,
    #[error("device not found")]
    DeviceNotFound,
    #[error("device is not active")]
    DeviceNotActive,
    #[error("active signing key is unavailable")]
    SigningKeyUnavailable,
    #[error("internal policy error: {0}")]
    Internal(String),
    #[error(transparent)]
    Database(#[from] DbErr),
}

pub async fn create_draft(
    db: &DatabaseConnection,
    admin_id: Uuid,
    input: PolicyDraftInput,
) -> Result<policy::Model, PolicyError> {
    let now = Utc::now();
    if input.expires_at <= now {
        return Err(PolicyError::InvalidRequest(
            "expires_at 必须晚于当前时间".into(),
        ));
    }
    let expires_at = input.expires_at;
    let payload = input.into_payload(0)?;
    let policy_json = serde_json::to_value(payload)
        .map_err(|error| PolicyError::Internal(format!("serialize policy: {error}")))?;
    Ok(policy::ActiveModel {
        id: Set(Uuid::new_v4()),
        version: Set(None),
        status: Set(PolicyStatus::Draft.as_str().into()),
        policy_json: Set(policy_json),
        signature_hex: Set(None),
        server_key_id: Set(None),
        expires_at: Set(expires_at),
        created_by_admin_id: Set(admin_id),
        created_at: Set(now),
        signed_at: Set(None),
        updated_at: Set(now),
    }
    .insert(db)
    .await?)
}

pub async fn sign_policy(
    db: &DatabaseConnection,
    signing_keys: &SigningKeyStore,
    policy_id: Uuid,
) -> Result<policy::Model, PolicyError> {
    let tx = db.begin().await?;
    let policy = policy::Entity::find_by_id(policy_id)
        .lock_exclusive()
        .one(&tx)
        .await?
        .ok_or(PolicyError::NotFound)?;
    let status = policy_status(&policy)?;
    if status != PolicyStatus::Draft {
        tx.rollback().await?;
        return Err(PolicyError::NotDraft);
    }
    let now = Utc::now();
    if policy.expires_at <= now {
        tx.rollback().await?;
        return Err(PolicyError::Expired);
    }

    let signing_key_record = signing_key::Entity::find()
        .filter(signing_key::Column::Status.eq("active"))
        .lock_exclusive()
        .one(&tx)
        .await?
        .ok_or(PolicyError::SigningKeyUnavailable)?;
    let signing_key = signing_keys
        .load(&signing_key_record.key_id)
        .map_err(|_| PolicyError::SigningKeyUnavailable)?;
    let expected_public_key = signing_key_record.public_key_hex.as_str();
    if anchor_core::crypto::verifying_key_to_hex(&signing_key.verifying_key())
        != expected_public_key
    {
        tx.rollback().await?;
        return Err(PolicyError::Internal(
            "signing key file does not match active database key".into(),
        ));
    }

    let version = next_policy_version(&tx).await?;
    let package = build_signed_package(
        policy.policy_json.clone(),
        policy.expires_at,
        version,
        &signing_key,
    )?;
    let policy_json = serde_json::to_value(&package.payload)
        .map_err(|error| PolicyError::Internal(format!("encode signed policy: {error}")))?;

    let mut active: policy::ActiveModel = policy.into();
    active.version = Set(Some(version));
    active.status = Set(PolicyStatus::Signed.as_str().into());
    active.policy_json = Set(policy_json);
    active.signature_hex = Set(Some(package.signature));
    active.server_key_id = Set(Some(signing_key_record.key_id));
    active.signed_at = Set(Some(now));
    active.updated_at = Set(now);
    let signed = active.update(&tx).await?;
    tx.commit().await?;
    Ok(signed)
}

pub async fn assign_policy(
    db: &DatabaseConnection,
    admin_id: Uuid,
    policy_id: Uuid,
    target: AssignmentTarget,
) -> Result<(), PolicyError> {
    let tx = db.begin().await?;
    let policy = policy::Entity::find_by_id(policy_id)
        .lock_exclusive()
        .one(&tx)
        .await?
        .ok_or(PolicyError::NotFound)?;
    if policy_status(&policy)? != PolicyStatus::Signed || policy.version.is_none() {
        tx.rollback().await?;
        return Err(PolicyError::NotSigned);
    }
    if policy.expires_at <= Utc::now() {
        tx.rollback().await?;
        return Err(PolicyError::Expired);
    }

    let now = Utc::now();
    match target {
        AssignmentTarget::Device(device_id) => {
            let device = device::Entity::find_by_id(device_id)
                .one(&tx)
                .await?
                .ok_or(PolicyError::DeviceNotFound)?;
            if status_of(&device).map_err(map_device_error)?
                != anchor_protocol::DeviceStatus::Active
            {
                tx.rollback().await?;
                return Err(PolicyError::DeviceNotActive);
            }
            if let Some(existing) = policy_assignment::Entity::find()
                .filter(policy_assignment::Column::DeviceId.eq(device_id))
                .one(&tx)
                .await?
            {
                let mut active: policy_assignment::ActiveModel = existing.into();
                active.policy_id = Set(policy_id);
                active.assigned_by_admin_id = Set(admin_id);
                active.created_at = Set(now);
                active.update(&tx).await?;
            } else {
                policy_assignment::ActiveModel {
                    id: Set(Uuid::new_v4()),
                    policy_id: Set(policy_id),
                    device_id: Set(Some(device_id)),
                    is_default: Set(false),
                    assigned_by_admin_id: Set(admin_id),
                    created_at: Set(now),
                }
                .insert(&tx)
                .await?;
            }
        }
        AssignmentTarget::Default => {
            if let Some(existing) = policy_assignment::Entity::find()
                .filter(policy_assignment::Column::IsDefault.eq(true))
                .one(&tx)
                .await?
            {
                let mut active: policy_assignment::ActiveModel = existing.into();
                active.policy_id = Set(policy_id);
                active.assigned_by_admin_id = Set(admin_id);
                active.created_at = Set(now);
                active.update(&tx).await?;
            } else {
                policy_assignment::ActiveModel {
                    id: Set(Uuid::new_v4()),
                    policy_id: Set(policy_id),
                    device_id: Set(None),
                    is_default: Set(true),
                    assigned_by_admin_id: Set(admin_id),
                    created_at: Set(now),
                }
                .insert(&tx)
                .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(())
}

pub async fn policy_for_device(
    db: &DatabaseConnection,
    device_id: Uuid,
) -> Result<Option<policy::Model>, PolicyError> {
    let assignment = match policy_assignment::Entity::find()
        .filter(policy_assignment::Column::DeviceId.eq(device_id))
        .one(db)
        .await?
    {
        Some(assignment) => Some(assignment),
        None => {
            policy_assignment::Entity::find()
                .filter(policy_assignment::Column::IsDefault.eq(true))
                .one(db)
                .await?
        }
    };
    let Some(assignment) = assignment else {
        return Ok(None);
    };
    let policy = policy::Entity::find_by_id(assignment.policy_id)
        .one(db)
        .await?;
    let Some(policy) = policy else {
        return Ok(None);
    };
    if policy_status(&policy)? != PolicyStatus::Signed
        || policy.version.is_none()
        || policy.expires_at <= Utc::now()
    {
        return Ok(None);
    }
    Ok(Some(policy))
}

/// 记录设备最近一次成功拉取到的策略；该记录用于管理台展示，不参与授权判断。
pub async fn record_policy_fetch(
    db: &DatabaseConnection,
    device_id: Uuid,
    version: u64,
) -> Result<(), PolicyError> {
    let version = i64::try_from(version)
        .map_err(|_| PolicyError::Internal("policy version exceeds i64".into()))?;
    device::Entity::update_many()
        .col_expr(
            device::Column::LastPolicyVersion,
            Expr::value(Some(version)),
        )
        .col_expr(
            device::Column::LastPolicyFetchedAt,
            Expr::value(Some(Utc::now())),
        )
        .filter(device::Column::Id.eq(device_id))
        .exec(db)
        .await?;
    Ok(())
}

pub fn envelope(policy: &policy::Model) -> Result<PolicyEnvelope, PolicyError> {
    let payload: PolicyPayload = serde_json::from_value(policy.policy_json.clone())
        .map_err(|error| PolicyError::Internal(format!("decode signed policy: {error}")))?;
    let version = policy
        .version
        .ok_or_else(|| PolicyError::Internal("signed policy has no version".into()))?;
    if payload.version != version as u64 || payload.expires_at != policy.expires_at {
        return Err(PolicyError::Internal(
            "signed policy metadata does not match its payload".into(),
        ));
    }
    Ok(PolicyEnvelope {
        version: version as u64,
        expires_at: policy.expires_at,
        policy_json: policy.policy_json.clone(),
        signature_hex: policy
            .signature_hex
            .clone()
            .ok_or_else(|| PolicyError::Internal("signed policy has no signature".into()))?,
        server_key_id: policy
            .server_key_id
            .clone()
            .ok_or_else(|| PolicyError::Internal("signed policy has no key id".into()))?,
    })
}

pub async fn keyring(db: &DatabaseConnection) -> Result<Vec<ServerKey>, PolicyError> {
    let keys = signing_key::Entity::find()
        .filter(signing_key::Column::Status.is_in([
            ServerKeyStatus::Active.as_str(),
            ServerKeyStatus::Retired.as_str(),
        ]))
        .order_by_asc(signing_key::Column::Status)
        .all(db)
        .await?;
    keys.into_iter()
        .map(|key| {
            let status = match key.status.as_str() {
                "active" => ServerKeyStatus::Active,
                "retired" => ServerKeyStatus::Retired,
                other => {
                    return Err(PolicyError::Internal(format!(
                        "unexpected keyring status {other}"
                    )));
                }
            };
            Ok(ServerKey {
                key_id: key.key_id,
                public_key_hex: key.public_key_hex,
                status,
            })
        })
        .collect()
}

pub fn policy_status(policy: &policy::Model) -> Result<PolicyStatus, PolicyError> {
    PolicyStatus::parse(&policy.status)
        .ok_or_else(|| PolicyError::Internal(format!("unknown policy status {}", policy.status)))
}

fn build_signed_package(
    policy_json: serde_json::Value,
    expires_at: DateTime<Utc>,
    version: i64,
    signing_key: &ed25519_dalek::SigningKey,
) -> Result<PolicyPackage, PolicyError> {
    if version <= 0 {
        return Err(PolicyError::Internal(
            "policy version must be positive".into(),
        ));
    }
    let mut payload: PolicyPayload = serde_json::from_value(policy_json)
        .map_err(|error| PolicyError::Internal(format!("decode policy draft: {error}")))?;
    payload.version = version as u64;
    payload.expires_at = expires_at;
    payload
        .validate()
        .map_err(|error| PolicyError::InvalidRequest(error.to_string()))?;
    PolicyPackage::sign(payload, signing_key)
        .map_err(|error| PolicyError::Internal(format!("sign policy: {error}")))
}

async fn next_policy_version(connection: &impl ConnectionTrait) -> Result<i64, PolicyError> {
    let row = connection
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT nextval('policy_version_seq') AS version",
        ))
        .await?
        .ok_or_else(|| PolicyError::Internal("policy version sequence returned no row".into()))?;
    let version = row
        .try_get::<i64>("", "version")
        .map_err(|error| PolicyError::Internal(format!("read policy version: {error}")))?;
    if version <= 0 {
        return Err(PolicyError::Internal(
            "policy version sequence returned a non-positive value".into(),
        ));
    }
    Ok(version)
}

fn map_device_error(error: DeviceError) -> PolicyError {
    PolicyError::Internal(format!("resolve device state: {error}"))
}

#[cfg(test)]
mod tests {
    use anchor_core::{crypto, ActionPolicy, UsbAllowRule};

    use super::*;

    fn draft(expires_at: DateTime<Utc>) -> PolicyDraftInput {
        PolicyDraftInput {
            expires_at,
            usb: UsbRules {
                read: ActionPolicy::Deny,
                write: ActionPolicy::Deny,
                allowed: vec![UsbAllowRule {
                    instance_id: Some("USBSTOR\\disk".into()),
                    vendor_id: None,
                    product_id: None,
                    serial: None,
                }],
            },
            network: NetworkRules {
                policy: ActionPolicy::Deny,
                allowed: vec![],
            },
            burning: BurningRules {
                policy: ActionPolicy::Deny,
                allowed: vec![],
            },
            scan: ScanRules {
                controlled_dirs: vec![],
                file_types: vec![],
                rules: vec![],
            },
        }
    }

    #[test]
    fn draft_payload_uses_zero_until_signing() {
        let payload = draft(Utc::now() + chrono::Duration::days(1))
            .into_payload(0)
            .unwrap();
        assert_eq!(payload.version, 0);
    }

    #[test]
    fn policy_status_has_a_stable_mapping() {
        assert_eq!(PolicyStatus::parse("draft"), Some(PolicyStatus::Draft));
        assert_eq!(PolicyStatus::parse("signed"), Some(PolicyStatus::Signed));
        assert_eq!(PolicyStatus::parse("other"), None);
    }

    #[test]
    fn signed_package_uses_the_server_version_and_verifies() {
        let (signing_key, verifying_key) = crypto::generate_keypair();
        let expires_at = Utc::now() + chrono::Duration::days(1);
        let payload = draft(expires_at).into_payload(0).unwrap();
        let package = build_signed_package(
            serde_json::to_value(payload).unwrap(),
            expires_at,
            7,
            &signing_key,
        )
        .unwrap();

        assert_eq!(package.payload.version, 7);
        assert_eq!(package.payload.expires_at, expires_at);
        assert!(package.verify_signature(&verifying_key));
    }
}
