use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "policies")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub version: Option<i64>,
    pub status: String,
    pub policy_json: Json,
    pub signature_hex: Option<String>,
    pub server_key_id: Option<String>,
    pub expires_at: DateTimeUtc,
    pub created_by_admin_id: Uuid,
    pub created_at: DateTimeUtc,
    pub signed_at: Option<DateTimeUtc>,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
