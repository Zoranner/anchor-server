use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "signing_keys")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub key_id: String,
    pub public_key_hex: String,
    pub status: String,
    pub created_by_admin_id: Uuid,
    pub created_at: DateTimeUtc,
    pub activated_at: DateTimeUtc,
    pub retired_at: Option<DateTimeUtc>,
    pub revoked_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
