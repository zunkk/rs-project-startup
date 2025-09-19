use chrono::{Local, NaiveDateTime, TimeZone};
use sea_orm::Set;
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::{Index, IndexCreateStatement};
use serde::{Deserialize, Serialize};

use crate::core::model::common::DeleteState;
use crate::core::model::common::DeleteState::Active;

pub fn create_index_statements() -> Vec<IndexCreateStatement> {
    vec![
        Index::create()
            .name("user_auth_type_index")
            .table(Entity::default().table_ref())
            .col(Column::AuthType)
            .col(Column::AuthId)
            .if_not_exists()
            .to_owned(),
        Index::create()
            .name("user_auth_user_id_index")
            .table(Entity::default().table_ref())
            .col(Column::UserId)
            .col(Column::AuthType)
            .if_not_exists()
            .to_owned(),
    ]
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
pub enum AuthType {
    #[sea_orm(string_value = "username")]
    Username,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_auth")]
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
    #[sea_orm(column_type = "String(StringLen::N(255))")]
    pub user_id: String,
    pub auth_type: AuthType,
    #[sea_orm(column_type = "String(StringLen::N(255))")]
    pub auth_id: String,
    #[sea_orm(column_type = "String(StringLen::N(1000))")]
    pub auth_token: String,
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
            user_id: Set("".into()),
            auth_type: Set(AuthType::Username),
            auth_id: Set("".into()),
            auth_token: Set("".into()),
        }
    }
}
