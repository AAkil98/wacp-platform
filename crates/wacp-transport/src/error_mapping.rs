use tonic::Code;
use wacp_types::ErrorCategory;

/// Map protocol error categories to gRPC status codes (protocol-interface spec §9).
pub fn error_category_to_grpc_code(category: ErrorCategory) -> Code {
    match category {
        ErrorCategory::PermissionDenied => Code::PermissionDenied,
        ErrorCategory::IllegalTransition => Code::FailedPrecondition,
        ErrorCategory::ValidationFailed => Code::InvalidArgument,
        ErrorCategory::BudgetExceeded => Code::ResourceExhausted,
        ErrorCategory::Timeout => Code::DeadlineExceeded,
        ErrorCategory::NotFound => Code::NotFound,
        ErrorCategory::DeliveryFailed => Code::Unavailable,
        ErrorCategory::Internal => Code::Internal,
    }
}

/// Create a tonic::Status from a protocol error.
pub fn protocol_error_to_status(error: &wacp_types::ProtocolError) -> tonic::Status {
    let code = error_category_to_grpc_code(error.category);
    tonic::Status::new(code, &error.message)
}
