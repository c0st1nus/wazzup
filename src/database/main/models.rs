use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, ToSchema)]
#[sea_orm(table_name = "companies")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub email: String,
    pub phone: Option<String>,
    pub database_name: String,
    pub wazzup_api_key: String,
    pub is_active: Option<bool>,
    // Подсказываем utoipa, как отображать этот тип в OpenAPI
    #[schema(value_type = String, format = DateTime)]
    pub created_at: Option<DateTimeUtc>,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: Option<DateTimeUtc>,
    pub subscription_tier: Option<String>,
    pub max_locations: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}