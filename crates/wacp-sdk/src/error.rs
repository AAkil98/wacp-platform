/// SDK-level errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("not connected")]
    NotConnected,

    #[error("bind failed: {0}")]
    BindFailed(String),

    #[error("rpc failed: {0}")]
    RpcFailed(#[from] tonic::Status),

    #[error("transport error: {0}")]
    Transport(#[from] tonic::transport::Error),

    #[error("stream ended")]
    StreamEnded,

    #[error("missing required field: {0}")]
    MissingField(String),
}
