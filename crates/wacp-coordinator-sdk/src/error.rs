/// Coordinator SDK errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("rpc failed: {0}")]
    RpcFailed(#[from] tonic::Status),

    #[error("transport error: {0}")]
    Transport(#[from] tonic::transport::Error),

    #[error("stream ended")]
    StreamEnded,

    #[error("invalid task graph: {0}")]
    InvalidGraph(String),

    #[error("workspace not found: {0}")]
    WorkspaceNotFound(String),

    #[error("task not ready: {0}")]
    TaskNotReady(String),

    #[error("signal wait timed out after {0}ms")]
    SignalTimeout(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        assert_eq!(
            Error::InvalidGraph("cycle detected".into()).to_string(),
            "invalid task graph: cycle detected"
        );
        assert_eq!(
            Error::SignalTimeout(5000).to_string(),
            "signal wait timed out after 5000ms"
        );
        assert_eq!(
            Error::WorkspaceNotFound("ws-99".into()).to_string(),
            "workspace not found: ws-99"
        );
    }
}
