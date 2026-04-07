//! Error types for the build agent.

/// Result type for agent operations.
pub type Result<T> = std::result::Result<T, AgentError>;

/// Errors that can occur in the build agent.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// Configuration error.
    #[error("configuration error: {0}")]
    Config(String),

    /// Failed to read configuration file.
    #[error("failed to read config file: {0}")]
    ConfigFile(#[from] std::io::Error),

    /// Failed to parse configuration.
    #[error("failed to parse config: {0}")]
    ConfigParse(#[from] toml::de::Error),

    /// Failed to parse configuration as YAML.
    #[error("failed to parse config as YAML: {0}")]
    ConfigParseYaml(#[from] serde_yaml::Error),

    /// gRPC error.
    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::Status),

    /// gRPC transport error.
    #[error("gRPC transport error: {0}")]
    GrpcTransport(#[from] tonic::transport::Error),

    /// NATS error.
    #[error("NATS error: {0}")]
    Nats(#[from] async_nats::Error),

    /// Registration failed.
    #[error("registration failed: {0}")]
    Registration(String),

    /// Agent not registered.
    #[error("agent not registered")]
    NotRegistered,

    /// Agent revoked.
    #[error("agent has been revoked")]
    Revoked,

    /// JWT expired.
    #[error("JWT expired")]
    JwtExpired,

    /// Job execution failed.
    #[error("job execution failed: {0}")]
    JobExecution(String),

    /// Step execution failed.
    #[error("step execution failed: {exit_code}: {message}")]
    StepExecution { exit_code: i32, message: String },

    /// Container runtime error.
    #[error("container runtime error: {0}")]
    ContainerRuntime(String),

    /// Process execution error.
    #[error("process execution error: {0}")]
    ProcessExecution(String),

    /// Secret decryption failed.
    #[error("secret decryption failed: {0}")]
    SecretDecryption(String),

    /// Secret verification failed (checksum mismatch).
    #[error("secret verification failed for key: {0}")]
    SecretVerification(String),

    /// General security error.
    #[error("security error: {0}")]
    Security(String),

    /// Workspace error.
    #[error("workspace error: {0}")]
    Workspace(String),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Certificate generation error.
    #[error("certificate error: {0}")]
    Certificate(#[from] rcgen::Error),

    /// Timeout error.
    #[error("operation timed out: {0}")]
    Timeout(String),

    /// Shutdown requested.
    #[error("shutdown requested")]
    Shutdown,

    /// Internal error.
    #[error("internal error: {0}")]
    Internal(String),

    /// Log shipping to controller failed or the stream closed unexpectedly.
    #[error("log stream: {0}")]
    LogStream(String),
}
