use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "devices")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub installation_id: Uuid,
    pub hostname: String,
    pub status: String,
    pub token_hash: Vec<u8>,
    pub token_issued_at: DateTimeUtc,
    pub approved_at: Option<DateTimeUtc>,
    pub approved_by_admin_id: Option<Uuid>,
    pub last_seen_at: Option<DateTimeUtc>,
    pub client_version: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub disabled_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
