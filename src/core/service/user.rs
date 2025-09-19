use std::sync::Arc;

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use sea_orm::{
    ActiveEnum, ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
};
use sidecar::prelude::*;
use sidecar::repo::Repo;
use sidecar::sidecar::Sidecar;

use crate::core::db::DB;
use crate::core::model::user::Role;
use crate::core::model::user_auth::{AuthType, Column};
use crate::core::model::{user, user_auth};
use crate::kit::config::Config;
use crate::kit::error::Error;

pub struct Service {
    _sidecar: Sidecar,
    _repo: Repo<Config>,
    pub db: Arc<DB>,
}

impl Service {
    pub async fn new(sidecar: Sidecar, repo: Repo<Config>, db: Arc<DB>) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            _sidecar: sidecar.with_component_name("user-service"),
            _repo: repo,
            db,
        }))
    }

    pub async fn create_tables(&self) -> Result<()> {
        self.db
            .create_table::<user::Entity>(user::create_index_statements())
            .await?;
        self.db
            .create_table::<user_auth::Entity>(user_auth::create_index_statements())
            .await?;
        Ok(())
    }

    pub async fn get_connection(&self) -> Result<DatabaseConnection> {
        self.db.get_connection().await
    }

    pub async fn register(
        &self,
        auth_type: AuthType,
        auth_id: String,
        auth_token: String,
        role: Role,
        name: String,
        desc: String,
    ) -> Result<String> {
        let conn = self.get_connection().await?;

        let auth_type_name = auth_type.to_value();
        let auth_id_for_error = auth_id.clone();

        let user_auth: Option<user_auth::Model> = user_auth::Entity::find()
            .filter(Column::AuthType.eq(auth_type.clone()))
            .filter(Column::AuthId.eq(auth_id.clone()))
            .one(&conn)
            .await?;

        if user_auth.is_some() {
            return Err(Error::UserAlreadyExists).wrap_err(format!(
                "auth_type: {}, auth_id: {}",
                auth_type_name, auth_id_for_error
            ));
        }

        let mut user = user::ActiveModel::create();
        user.role = Set(role);
        user.name = Set(name);
        user.desc = Set(desc);

        let user_id = user.id.clone().unwrap();

        let mut user_auth = user_auth::ActiveModel::create();
        user_auth.user_id = Set(user_id.clone());
        user_auth.auth_type = Set(auth_type.clone());
        user_auth.auth_id = Set(auth_id.clone());

        match auth_type {
            AuthType::Username => {
                // hash password
                user_auth.auth_token = Set(hash_password(&auth_token)?);
            }
        }

        let txn = conn.begin().await?;
        user.insert(&txn).await?;
        user_auth.insert(&txn).await?;
        txn.commit().await?;

        Ok(user_id)
    }

    pub async fn login(
        &self,
        auth_type: AuthType,
        auth_id: String,
        auth_token: String,
    ) -> Result<String> {
        let conn = self.get_connection().await?;

        let auth_type_name = auth_type.to_value();
        let auth_id_for_error = auth_id.clone();

        let user_auth: Option<user_auth::Model> = user_auth::Entity::find()
            .filter(Column::AuthType.eq(auth_type))
            .filter(Column::AuthId.eq(auth_id.clone()))
            .one(&conn)
            .await?;

        let Some(user_auth) = user_auth else {
            return Err(Error::UserNotFound).wrap_err(format!(
                "auth_type: {}, auth_id: {}",
                auth_type_name, auth_id_for_error
            ));
        };

        if !verify_password(&auth_token, &user_auth.auth_token) {
            return Err(Error::UserInvalidPassword).wrap_err(format!(
                "auth_type: {}, auth_id: {}",
                auth_type_name, auth_id_for_error
            ));
        }

        Ok(user_auth.user_id.clone())
    }

    pub async fn info(&self, user_id: String) -> Result<user::Model> {
        let conn = self.get_connection().await?;
        if let Some(res) = user::Entity::find_by_id(user_id.clone()).one(&conn).await? {
            Ok(res)
        } else {
            Err(Error::UserNotFound).wrap_err(format!("user_id: {}", user_id))
        }
    }
}

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::try_from_rng(&mut OsRng)?;
    let hash = Argon2::default().hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    if let Ok(parsed_hash) = PasswordHash::new(hash) {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok()
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hash_and_verify() {
        let password = "my-secret-password";

        let hashed = hash_password(password).unwrap();

        assert!(verify_password(password, &hashed));
        assert!(!verify_password("wrong-password", &hashed));
    }
}
