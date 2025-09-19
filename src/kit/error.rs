use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum Error {
    // -------------- system --------------
    #[error("Unknown error: {0}")]
    Unknown(String),

    #[error("Invalid request parameter: {0}")]
    InvidRequestParameter(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Api must request from ipc")]
    ApiMustRequestFromIPC,

    #[error("Db connection not initialized")]
    DBConnectionNotInitialized,

    // -------------- user --------------
    #[error("User not found")]
    UserNotFound,

    #[error("User already exists")]
    UserAlreadyExists,

    #[error("User invalid password")]
    UserInvalidPassword,
}

impl Error {
    pub fn code(&self) -> u64 {
        match self {
            // -------------- system --------------
            Error::Unknown(_) => 10001,
            Error::InvidRequestParameter(_) => 10002,
            Error::Unauthorized => 10003,
            Error::ApiMustRequestFromIPC => 10004,
            Error::DBConnectionNotInitialized => 10005,

            // -------------- user --------------
            Error::UserNotFound => 10101,
            Error::UserAlreadyExists => 10002,
            Error::UserInvalidPassword => 10003,
        }
    }
}
