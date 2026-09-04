use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("ADB error: {0}")]
    Adb(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("decryption failed: {0}")]
    Decrypt(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
