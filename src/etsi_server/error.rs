use hyper::StatusCode;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EtsiServerError {
    #[error("io error")]
    IoError(#[from] std::io::Error),
    #[error("uri error")]
    UriError(#[from] axum::http::uri::InvalidUri),
    #[error("tls error")]
    TlsError(#[from] native_tls::Error),
    #[error("client error")]
    ClientError(#[from] hyper_util::client::legacy::Error),
    #[error("axum error")]
    AxumError(#[from] axum::Error),
    #[error("http error")]
    HttpError(#[from] axum::http::Error),
    #[error("serde_json error")]
    SerdeJsonError(#[from] serde_json::Error),
    #[error("Server dont't have data about pqkd with sae_id {0}")]
    UnknownPqkd(String),
    #[error("Error")]
    PathError,
    #[error("Error")]
    SendKeysError,
    #[error("Failed pqkd request: statuscode - {0}")]
    PqkdRequestError(StatusCode),
    #[error("Error")]
    GetKeysError,
}

impl From<EtsiServerError> for StatusCode {
    fn from(_val: EtsiServerError) -> Self {
        //match val {
        //    _ => StatusCode::INTERNAL_SERVER_ERROR,
        //}
        StatusCode::INTERNAL_SERVER_ERROR
    }
}
