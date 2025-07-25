use std::fmt;
use thiserror::Error;
use tonic::Status;

/// 矿池服务器错误类型
#[derive(Error, Debug)]
pub enum PoolServerError {
    /// 网络相关错误
    #[error("网络错误: {0}")]
    NetworkError(String),
    
    /// 协议相关错误
    #[error("协议错误: {0}")]
    ProtocolError(String),
    
    /// 数据处理错误
    #[error("数据错误: {0}")]
    DataError(String),
    
    /// 区块链同步错误
    #[error("同步错误: {0}")]
    SyncError(String),
    
    /// 内部服务错误
    #[error("服务错误: {0}")]
    ServiceError(String),
    
    /// 其他错误
    #[error("其他错误: {0}")]
    Other(String),
}

impl PoolServerError {
    /// 将错误转换为日志友好的格式
    pub fn to_log_string(&self) -> String {
        match self {
            Self::NetworkError(msg) => format!("网络错误: {}", msg),
            Self::ProtocolError(msg) => format!("协议错误: {}", msg),
            Self::DataError(msg) => format!("数据错误: {}", msg),
            Self::SyncError(msg) => format!("同步错误: {}", msg),
            Self::ServiceError(msg) => format!("服务错误: {}", msg),
            Self::Other(msg) => format!("其他错误: {}", msg),
        }
    }
    
    /// 获取错误的严重程度
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::NetworkError(_) => ErrorSeverity::Warning,
            Self::ProtocolError(_) => ErrorSeverity::Error,
            Self::DataError(_) => ErrorSeverity::Warning,
            Self::SyncError(_) => ErrorSeverity::Error,
            Self::ServiceError(_) => ErrorSeverity::Critical,
            Self::Other(_) => ErrorSeverity::Warning,
        }
    }
    
    /// 判断错误是否可恢复
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::NetworkError(_) => true,
            Self::ProtocolError(_) => false,
            Self::DataError(_) => true,
            Self::SyncError(_) => true,
            Self::ServiceError(_) => false,
            Self::Other(_) => false,
        }
    }
    
    /// 从anyhow错误转换
    pub fn from_anyhow(err: anyhow::Error) -> Self {
        // 尝试从错误信息判断错误类型
        let err_string = err.to_string();
        
        if err_string.contains("network") || err_string.contains("连接") || 
           err_string.contains("connection") || err_string.contains("timeout") || 
           err_string.contains("超时") {
            Self::NetworkError(err_string)
        } else if err_string.contains("protocol") || err_string.contains("协议") {
            Self::ProtocolError(err_string)
        } else if err_string.contains("data") || err_string.contains("数据") {
            Self::DataError(err_string)
        } else if err_string.contains("sync") || err_string.contains("同步") {
            Self::SyncError(err_string)
        } else {
            Self::Other(err_string)
        }
    }
}

/// 错误严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// 信息性的，不需要特别处理
    Info,
    /// 警告，可能需要注意
    Warning,
    /// 错误，需要处理
    Error,
    /// 严重错误，需要立即处理
    Critical,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARNING"),
            Self::Error => write!(f, "ERROR"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// 从PoolServerError转换为tonic::Status
impl From<PoolServerError> for Status {
    fn from(error: PoolServerError) -> Self {
        match error {
            PoolServerError::NetworkError(msg) => Status::unavailable(msg),
            PoolServerError::ProtocolError(msg) => Status::invalid_argument(msg),
            PoolServerError::DataError(msg) => Status::data_loss(msg),
            PoolServerError::SyncError(msg) => Status::failed_precondition(msg),
            PoolServerError::ServiceError(msg) => Status::internal(msg),
            PoolServerError::Other(msg) => Status::unknown(msg),
        }
    }
}

/// 结果类型别名
pub type PoolResult<T> = Result<T, PoolServerError>; 