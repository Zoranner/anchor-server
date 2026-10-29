use anchor_protocol::{
    DeviceStatus, HardwareInventory, RegisterDeviceRequest, HARDWARE_INVENTORY_SCHEMA_VERSION,
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QueryOrder, Set, TransactionTrait,
};
use sea_query::Expr;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    auth,
    entity::{device, device_inventory_snapshot},
};

pub const MAX_INVENTORY_BYTES: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error("device already active")]
    AlreadyActive,
    #[error("device rejected")]
    Rejected,
    #[error("device disabled")]
    Disabled,
    #[error("device pending")]
    Pending,
    #[error("device not found")]
    NotFound,
    #[error("device token invalid")]
    TokenInvalid,
    #[error("invalid device state")]
    InvalidState,
    #[error("device inventory changed")]
    InventoryChanged,
    #[error("internal device error: {0}")]
    Internal(String),
    #[error(transparent)]
    Database(#[from] DbErr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredDevice {
    pub device_id: Uuid,
    pub status: DeviceStatus,
    pub token: String,
}

pub async fn register_device(
    db: &DatabaseConnection,
    request: &RegisterDeviceRequest,
) -> Result<RegisteredDevice, DeviceError> {
    let installation_id = Uuid::parse_str(&request.installation_id)
        .map_err(|_| DeviceError::InvalidRequest("installation_id must be a UUID".into()))?;
    let hostname = validate_text(&request.hostname, "hostname", 255)?;
    let client_version = validate_text(&request.client_version, "client_version", 64)?;
    if request.inventory.schema_version != HARDWARE_INVENTORY_SCHEMA_VERSION {
        return Err(DeviceError::InvalidRequest(
            "unsupported hardware inventory schema_version".into(),
        ));
    }
    let inventory = PreparedInventory::new(&request.inventory)?;

    let tx = db.begin().await?;
    let lock_key = installation_lock_key(installation_id);
    tx.execute_unprepared(&format!("SELECT pg_advisory_xact_lock({lock_key})"))
        .await?;

    let existing = device::Entity::find()
        .filter(device::Column::InstallationId.eq(installation_id))
        .one(&tx)
        .await?;
    let now = Utc::now();

    let Some(existing) = existing else {
        let token = auth::generate_secret();
        let device = device::ActiveModel {
            id: Set(Uuid::new_v4()),
            installation_id: Set(installation_id),
            hostname: Set(hostname),
            status: Set(DeviceStatus::Pending.as_str().into()),
            token_hash: Set(auth::secret_hash(&token)),
            token_issued_at: Set(now),
            approved_at: Set(None),
            approved_by_admin_id: Set(None),
            last_seen_at: Set(None),
            client_version: Set(Some(client_version)),
            created_at: Set(now),
            updated_at: Set(now),
            disabled_at: Set(None),
        }
        .insert(&tx)
        .await?;
        inventory.insert(&tx, device.id, now).await?;
        tx.commit().await?;
        return Ok(RegisteredDevice {
            device_id: device.id,
            status: DeviceStatus::Pending,
            token,
        });
    };

    let status = status_of(&existing)?;
    if status != DeviceStatus::Pending {
        tx.rollback().await?;
        return Err(match status {
            DeviceStatus::Active => DeviceError::AlreadyActive,
            DeviceStatus::Rejected => DeviceError::Rejected,
            DeviceStatus::Disabled => DeviceError::Disabled,
            DeviceStatus::Pending => unreachable!(),
        });
    }

    let token = auth::generate_secret();
    let device_id = existing.id;
    let mut active: device::ActiveModel = existing.into();
    active.hostname = Set(hostname);
    active.client_version = Set(Some(client_version));
    active.token_hash = Set(auth::secret_hash(&token));
    active.token_issued_at = Set(now);
    active.updated_at = Set(now);
    active.update(&tx).await?;
    inventory.insert(&tx, device_id, now).await?;
    tx.commit().await?;

    Ok(RegisteredDevice {
        device_id,
        status: DeviceStatus::Pending,
        token,
    })
}

pub async fn find_device_by_token(
    db: &DatabaseConnection,
    token: &str,
) -> Result<device::Model, DeviceError> {
    if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(DeviceError::TokenInvalid);
    }
    device::Entity::find()
        .filter(device::Column::TokenHash.eq(auth::secret_hash(token)))
        .one(db)
        .await?
        .ok_or(DeviceError::TokenInvalid)
}

pub fn ensure_active(device: &device::Model) -> Result<(), DeviceError> {
    match status_of(device)? {
        DeviceStatus::Active => Ok(()),
        DeviceStatus::Pending => Err(DeviceError::Pending),
        DeviceStatus::Rejected => Err(DeviceError::Rejected),
        DeviceStatus::Disabled => Err(DeviceError::Disabled),
    }
}

pub async fn heartbeat(
    db: &DatabaseConnection,
    device: &device::Model,
    client_version: &str,
    inventory_hash: &str,
    inventory: Option<&HardwareInventory>,
) -> Result<(), DeviceError> {
    ensure_active(device)?;
    let client_version = validate_text(client_version, "client_version", 64)?;
    let provided_hash = if inventory_hash.len() == 64 {
        hex::decode(inventory_hash).map_err(|_| {
            DeviceError::InvalidRequest("inventory_hash must be a SHA-256 hex digest".into())
        })?
    } else {
        return Err(DeviceError::InvalidRequest(
            "inventory_hash must be a SHA-256 hex digest".into(),
        ));
    };
    let latest_hash = device_inventory_snapshot::Entity::find()
        .filter(device_inventory_snapshot::Column::DeviceId.eq(device.id))
        .order_by_desc(device_inventory_snapshot::Column::CreatedAt)
        .order_by_desc(device_inventory_snapshot::Column::Id)
        .one(db)
        .await?
        .map(|snapshot| snapshot.inventory_hash);
    if latest_hash.as_deref() != Some(provided_hash.as_slice()) {
        let Some(inventory) = inventory else {
            return Err(DeviceError::InventoryChanged);
        };
        if inventory.schema_version != HARDWARE_INVENTORY_SCHEMA_VERSION {
            return Err(DeviceError::InvalidRequest(
                "unsupported hardware inventory schema_version".into(),
            ));
        }
        let prepared = PreparedInventory::new(inventory)?;
        if prepared.hash != provided_hash {
            return Err(DeviceError::InvalidRequest(
                "inventory_hash does not match the submitted inventory".into(),
            ));
        }
        prepared.insert(db, device.id, Utc::now()).await?;
    }

    let now = Utc::now();
    let result = device::Entity::update_many()
        .col_expr(device::Column::LastSeenAt, Expr::value(now))
        .col_expr(device::Column::ClientVersion, Expr::value(client_version))
        .col_expr(device::Column::UpdatedAt, Expr::value(now))
        .filter(device::Column::Id.eq(device.id))
        .filter(device::Column::Status.eq(DeviceStatus::Active.as_str()))
        .exec(db)
        .await?;
    if result.rows_affected == 0 {
        return Err(DeviceError::InvalidState);
    }
    Ok(())
}

pub async fn approve_device(
    db: &DatabaseConnection,
    device_id: Uuid,
    admin_id: Uuid,
) -> Result<(), DeviceError> {
    let now = Utc::now();
    let result = device::Entity::update_many()
        .col_expr(
            device::Column::Status,
            Expr::value(DeviceStatus::Active.as_str()),
        )
        .col_expr(device::Column::ApprovedAt, Expr::value(Some(now)))
        .col_expr(
            device::Column::ApprovedByAdminId,
            Expr::value(Some(admin_id)),
        )
        .col_expr(
            device::Column::DisabledAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(device::Column::UpdatedAt, Expr::value(now))
        .filter(device::Column::Id.eq(device_id))
        .filter(device::Column::Status.eq(DeviceStatus::Pending.as_str()))
        .exec(db)
        .await?;
    if result.rows_affected == 0 {
        return transition_failed(db, device_id).await;
    }
    Ok(())
}

pub async fn reject_device(db: &DatabaseConnection, device_id: Uuid) -> Result<(), DeviceError> {
    let now = Utc::now();
    let result = device::Entity::update_many()
        .col_expr(
            device::Column::Status,
            Expr::value(DeviceStatus::Rejected.as_str()),
        )
        .col_expr(
            device::Column::ApprovedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            device::Column::ApprovedByAdminId,
            Expr::value(Option::<Uuid>::None),
        )
        .col_expr(
            device::Column::DisabledAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(device::Column::UpdatedAt, Expr::value(now))
        .filter(device::Column::Id.eq(device_id))
        .filter(device::Column::Status.eq(DeviceStatus::Pending.as_str()))
        .exec(db)
        .await?;
    if result.rows_affected == 0 {
        return transition_failed(db, device_id).await;
    }
    Ok(())
}

pub async fn disable_device(db: &DatabaseConnection, device_id: Uuid) -> Result<(), DeviceError> {
    let now = Utc::now();
    let result = device::Entity::update_many()
        .col_expr(
            device::Column::Status,
            Expr::value(DeviceStatus::Disabled.as_str()),
        )
        .col_expr(device::Column::TokenHash, Expr::value(invalid_token_hash()))
        .col_expr(device::Column::TokenIssuedAt, Expr::value(now))
        .col_expr(device::Column::DisabledAt, Expr::value(Some(now)))
        .col_expr(device::Column::UpdatedAt, Expr::value(now))
        .filter(device::Column::Id.eq(device_id))
        .filter(device::Column::Status.is_in([
            DeviceStatus::Pending.as_str(),
            DeviceStatus::Active.as_str(),
        ]))
        .exec(db)
        .await?;
    if result.rows_affected == 0 {
        return transition_failed(db, device_id).await;
    }
    Ok(())
}

pub async fn reset_device_token(
    db: &DatabaseConnection,
    device_id: Uuid,
) -> Result<(), DeviceError> {
    let now = Utc::now();
    let result = device::Entity::update_many()
        .col_expr(
            device::Column::Status,
            Expr::value(DeviceStatus::Pending.as_str()),
        )
        .col_expr(device::Column::TokenHash, Expr::value(invalid_token_hash()))
        .col_expr(device::Column::TokenIssuedAt, Expr::value(now))
        .col_expr(
            device::Column::ApprovedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            device::Column::ApprovedByAdminId,
            Expr::value(Option::<Uuid>::None),
        )
        .col_expr(
            device::Column::DisabledAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(device::Column::UpdatedAt, Expr::value(now))
        .filter(device::Column::Id.eq(device_id))
        .exec(db)
        .await?;
    if result.rows_affected == 0 {
        return Err(DeviceError::NotFound);
    }
    Ok(())
}

pub fn status_of(device: &device::Model) -> Result<DeviceStatus, DeviceError> {
    DeviceStatus::parse(&device.status)
        .ok_or_else(|| DeviceError::Internal(format!("unknown device status {}", device.status)))
}

fn validate_text(value: &str, field: &str, max_len: usize) -> Result<String, DeviceError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(DeviceError::InvalidRequest(format!(
            "{field} must not be empty"
        )));
    }
    if value.len() > max_len {
        return Err(DeviceError::InvalidRequest(format!(
            "{field} must not exceed {max_len} bytes"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(DeviceError::InvalidRequest(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(value.into())
}

fn installation_lock_key(installation_id: Uuid) -> i64 {
    let bytes = installation_id.as_bytes();
    i64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ])
}

fn invalid_token_hash() -> Vec<u8> {
    auth::secret_hash(&auth::generate_secret())
}

async fn transition_failed(db: &DatabaseConnection, device_id: Uuid) -> Result<(), DeviceError> {
    if device::Entity::find_by_id(device_id)
        .one(db)
        .await?
        .is_none()
    {
        Err(DeviceError::NotFound)
    } else {
        Err(DeviceError::InvalidState)
    }
}

struct PreparedInventory {
    captured_at: DateTime<Utc>,
    json: serde_json::Value,
    hash: Vec<u8>,
}

impl PreparedInventory {
    fn new(inventory: &HardwareInventory) -> Result<Self, DeviceError> {
        let encoded = serde_json::to_vec(inventory)
            .map_err(|error| DeviceError::Internal(format!("serialize inventory: {error}")))?;
        if encoded.len() > MAX_INVENTORY_BYTES {
            return Err(DeviceError::InvalidRequest(format!(
                "inventory must not exceed {MAX_INVENTORY_BYTES} bytes"
            )));
        }
        Ok(Self {
            captured_at: inventory.captured_at,
            json: serde_json::to_value(inventory)
                .map_err(|error| DeviceError::Internal(format!("encode inventory: {error}")))?,
            hash: Sha256::digest(&encoded).to_vec(),
        })
    }

    async fn insert<C: ConnectionTrait>(
        self,
        connection: &C,
        device_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), DbErr> {
        device_inventory_snapshot::ActiveModel {
            id: Set(Uuid::new_v4()),
            device_id: Set(device_id),
            captured_at: Set(self.captured_at),
            inventory_json: Set(self.json),
            inventory_hash: Set(self.hash),
            created_at: Set(now),
        }
        .insert(connection)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_validation_trims_and_rejects_invalid_values() {
        assert_eq!(
            validate_text("  workstation  ", "hostname", 255).unwrap(),
            "workstation"
        );
        assert!(validate_text("   ", "hostname", 255).is_err());
        assert!(validate_text("host\nname", "hostname", 255).is_err());
        assert!(validate_text(&"a".repeat(256), "hostname", 255).is_err());
    }

    #[test]
    fn installation_lock_key_is_stable() {
        let id = Uuid::parse_str("3f0f5c1e-6d1a-4dd0-9d32-9e5b2a1c8f44").unwrap();
        assert_eq!(installation_lock_key(id), installation_lock_key(id));
    }
}
