use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ErrorCode {
    Success = 0,
    Internal = 1,
    Usage = 2,
    Schema = 3,
    NotFound = 4,
    RepoInvalid = 5,
    LockContention = 6,
    Conflict = 7,
    Io = 8,
    Permission = 9,
    Config = 10,
    Index = 11,
    ImportExport = 12,
    ExternalCommand = 13,
    Ai = 14,
    Interrupted = 15,
}

impl ErrorCode {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

#[derive(Debug)]
pub enum TikError {
    Usage(String),
    Schema(String),
    NotFound(String),
    RepoInvalid(String),
    LockContention(String),
    Conflict(String),
    Io {
        context: String,
        source: std::io::Error,
    },
    Permission(String),
    Config(String),
    Index(String),
    ImportExport(String),
    ExternalCommand(String),
    Ai(String),
    Interrupted,
    Internal(String),
}

impl TikError {
    pub fn code(&self) -> ErrorCode {
        match self {
            TikError::Internal(_) => ErrorCode::Internal,
            TikError::Usage(_) => ErrorCode::Usage,
            TikError::Schema(_) => ErrorCode::Schema,
            TikError::NotFound(_) => ErrorCode::NotFound,
            TikError::RepoInvalid(_) => ErrorCode::RepoInvalid,
            TikError::LockContention(_) => ErrorCode::LockContention,
            TikError::Conflict(_) => ErrorCode::Conflict,
            TikError::Io { .. } => ErrorCode::Io,
            TikError::Permission(_) => ErrorCode::Permission,
            TikError::Config(_) => ErrorCode::Config,
            TikError::Index(_) => ErrorCode::Index,
            TikError::ImportExport(_) => ErrorCode::ImportExport,
            TikError::ExternalCommand(_) => ErrorCode::ExternalCommand,
            TikError::Ai(_) => ErrorCode::Ai,
            TikError::Interrupted => ErrorCode::Interrupted,
        }
    }

    pub fn io(context: &str, source: std::io::Error) -> Self {
        TikError::Io {
            context: context.to_string(),
            source,
        }
    }

    pub fn internal(message: &str) -> Self {
        TikError::Internal(message.to_string())
    }

    pub fn usage(message: &str) -> Self {
        TikError::Usage(message.to_string())
    }

    pub fn repo_invalid(message: &str) -> Self {
        TikError::RepoInvalid(message.to_string())
    }
}

impl fmt::Display for TikError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TikError::Usage(msg) => write!(f, "usage error: {msg}"),
            TikError::Schema(msg) => write!(f, "schema validation error: {msg}"),
            TikError::NotFound(msg) => write!(f, "not found: {msg}"),
            TikError::RepoInvalid(msg) => write!(f, "repo invalid: {msg}"),
            TikError::LockContention(msg) => write!(f, "lock contention: {msg}"),
            TikError::Conflict(msg) => write!(f, "conflict detected: {msg}"),
            TikError::Io { context, source } => write!(f, "io error: {context}: {source}"),
            TikError::Permission(msg) => write!(f, "permission denied: {msg}"),
            TikError::Config(msg) => write!(f, "config error: {msg}"),
            TikError::Index(msg) => write!(f, "index error: {msg}"),
            TikError::ImportExport(msg) => write!(f, "import/export error: {msg}"),
            TikError::ExternalCommand(msg) => write!(f, "external command failed: {msg}"),
            TikError::Ai(msg) => write!(f, "ai backend error: {msg}"),
            TikError::Interrupted => write!(f, "interrupted"),
            TikError::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for TikError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TikError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, TikError>;
