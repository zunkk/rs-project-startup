use chrono::{Duration, Local};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sidecar::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: DeserializeOwned", serialize = "T: Serialize"))]
pub struct Claims<T>
where
    T: Serialize + DeserializeOwned,
{
    pub sub: String,
    pub exp: i64,
    pub nbf: i64,
    pub data: T,
}

impl<T> Default for Claims<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    fn default() -> Self {
        Self {
            sub: String::new(),
            exp: 0,
            nbf: 0,
            data: T::default(),
        }
    }
}

pub fn generate_with_hmac_key<T>(
    hmac_key: impl AsRef<[u8]>,
    valid_duration: Duration,
    id: &str,
    data: T,
) -> Result<(String, i64)>
where
    T: Serialize + DeserializeOwned,
{
    let now = Local::now();
    let exp_time = now + valid_duration;

    let claims = Claims {
        sub: id.to_string(),
        exp: exp_time.timestamp(),
        nbf: now.timestamp(),
        data,
    };

    let header = Header::new(Algorithm::HS256);
    let token = encode(
        &header,
        &claims,
        &EncodingKey::from_secret(hmac_key.as_ref()),
    )?;

    Ok((token, exp_time.timestamp()))
}

pub fn parse_with_hmac_key<T>(hmac_key: impl AsRef<[u8]>, token: &str) -> Result<(String, T)>
where
    T: Clone + Serialize + DeserializeOwned,
{
    let validation = Validation::new(Algorithm::HS256);
    let token_data = decode::<Claims<T>>(
        token,
        &DecodingKey::from_secret(hmac_key.as_ref()),
        &validation,
    )?;
    Ok((token_data.claims.sub.clone(), token_data.claims.data))
}
