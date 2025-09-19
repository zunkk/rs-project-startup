use chrono::{Local, NaiveDateTime, TimeZone};
use sea_orm::Set;
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::IndexCreateStatement;
use serde::{Deserialize, Serialize};

use crate::core::model::common::DeleteState;
use crate::core::model::common::DeleteState::Active;

pub fn create_index_statements() -> Vec<IndexCreateStatement> {
    vec![]
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))")]
pub enum Status {
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "inactive")]
    Inactive,
    #[sea_orm(string_value = "frozen")]
    Frozen,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))")]
pub enum Role {
    #[sea_orm(string_value = "admin")]
    Admin,
    #[sea_orm(string_value = "manager")]
    Manager,
    #[sea_orm(string_value = "user")]
    User,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user")]
pub struct Model {
    #[sea_orm(
        primary_key,
        column_type = "String(StringLen::N(255))",
        auto_increment = false
    )]
    pub id: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
    pub delete_time: DateTimeWithTimeZone,
    pub del_state: DeleteState,
    pub version: i64,
    #[sea_orm(column_type = "String(StringLen::N(20))")]
    pub status: Status,
    #[sea_orm(column_type = "String(StringLen::N(20))")]
    pub role: Role,
    #[sea_orm(column_type = "String(StringLen::N(255))")]
    pub name: String,
    #[sea_orm(column_type = "String(StringLen::N(1000))")]
    pub desc: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl ActiveModel {
    pub fn create() -> Self {
        let now = Local::now().into();
        Self {
            id: Set(Uuid::new_v4().simple().to_string()),
            create_time: Set(now),
            update_time: Set(now),
            delete_time: Set(Local.from_utc_datetime(&NaiveDateTime::default()).into()),
            del_state: Set(Active),
            version: Set(0),
            status: Set(Status::Active),
            role: Set(Role::User),
            name: Set("".into()),
            desc: Set("".into()),
        }
    }
}
