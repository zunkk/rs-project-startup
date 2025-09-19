use sea_orm::{DeriveActiveEnum, EnumIter};
use serde::{Deserialize, Serialize};

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
#[sea_orm(rs_type = "i32", db_type = "Integer")]
pub enum DeleteState {
    #[sea_orm(num_value = 0)]
    Active,
    #[sea_orm(num_value = 1)]
    Deleted,
}
