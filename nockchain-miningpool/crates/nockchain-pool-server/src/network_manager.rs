use std::time::Duration;
use tokio::time::timeout;
use tracing::{info, warn, error, debug};
use crate::error::{PoolServerError, PoolResult};
use regex::Regex;
use std::sync::Arc;
use crate::status_monitor::StatusMonitor;

/// 网络重试配置
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// 最大重试次数
    pub max_attempts: u32,
    /// 初始超时时间（毫秒）
    pub initial_timeout_ms: u64,
    /// 重试间隔时间（毫秒）
    pub retry_interval_ms: u64,
    /// 是否使用指数退避
    pub use_exponential_backoff: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_timeout_ms: 5000,
            retry_interval_ms: 1000,
            use_exponential_backoff: true,
        }
    }
}

/// 网络管理器
pub struct NetworkManager {
    retry_config: RetryConfig,
    status_monitor: Option<Arc<StatusMonitor>>,
}

impl NetworkManager {
    /// 创建新的网络管理器
    pub fn new(retry_config: RetryConfig) -> Self {
        Self { 
            retry_config,
            status_monitor: None,
        }
    }

    /// 使用默认配置创建网络管理器
    pub fn default() -> Self {
        Self::new(RetryConfig::default())
    }
    
    /// 设置状态监控器
    pub fn set_status_monitor(&mut self, status_monitor: Arc<StatusMonitor>) {
        self.status_monitor = Some(status_monitor);
    }

    /// 改进的重试异步操作方法，增加了更多错误处理和日志记录
    pub async fn enhanced_retry_async_operation<F, Fut, T>(&self, operation_name: &str, operation: F) -> PoolResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, anyhow::Error>>,
    {
        let mut last_error = None;
        let mut backoff_ms = self.retry_config.initial_timeout_ms;
        
        for attempt in 1..=self.retry_config.max_attempts {
            // 计算当前尝试的超时时间
            let timeout_ms = if self.retry_config.use_exponential_backoff {
                backoff_ms
            } else {
                self.retry_config.initial_timeout_ms
            };
            
            info!("执行操作 '{}' (尝试 {}/{}，超时 {}ms)",
                operation_name, attempt, self.retry_config.max_attempts, timeout_ms);
            
            // 使用超时执行操作
            match timeout(Duration::from_millis(timeout_ms), operation()).await {
                Ok(Ok(result)) => {
                    if attempt > 1 {
                        info!("操作 '{}' 在第 {} 次尝试后成功", operation_name, attempt);
                    } else {
                        debug!("操作 '{}' 首次尝试成功", operation_name);
                    }
                    return Ok(result);
                },
                Ok(Err(e)) => {
                    let err = PoolServerError::from_anyhow(e);
                    
                    // 根据错误严重程度选择日志级别
                    match err.severity() {
                        crate::error::ErrorSeverity::Critical | crate::error::ErrorSeverity::Error => {
                            error!("操作 '{}' 失败 (尝试 {}/{}): {}", 
                                operation_name, attempt, self.retry_config.max_attempts, err.to_log_string());
                        },
                        _ => {
                            warn!("操作 '{}' 失败 (尝试 {}/{}): {}", 
                                operation_name, attempt, self.retry_config.max_attempts, err.to_log_string());
                        }
                    }
                    
                    // 如果错误不可恢复，立即返回
                    if !err.is_recoverable() {
                        info!("操作 '{}' 遇到不可恢复的错误，停止重试", operation_name);
                        return Err(err);
                    }
                    
                    last_error = Some(err);
                },
                Err(_) => {
                    let err = PoolServerError::NetworkError(format!("操作 '{}' 超时 ({}ms)", operation_name, timeout_ms));
                    warn!("{}", err.to_log_string());
                    last_error = Some(err);
                }
            }
            
            // 如果不是最后一次尝试，则等待一段时间后重试
            if attempt < self.retry_config.max_attempts {
                let wait_time = if self.retry_config.use_exponential_backoff {
                    // 更新下一次的退避时间
                    let wait = self.retry_config.retry_interval_ms * attempt as u64;
                    backoff_ms *= 2; // 指数增长
                    wait
                } else {
                    self.retry_config.retry_interval_ms
                };
                
                info!("等待 {}ms 后重试操作 '{}'...", wait_time, operation_name);
                tokio::time::sleep(Duration::from_millis(wait_time)).await;
            }
        }
        
        // 所有尝试都失败，返回最后一个错误
        let final_error = last_error.unwrap_or_else(|| 
            PoolServerError::NetworkError(format!("操作 '{}' 失败，未知错误", operation_name))
        );
        
        error!("操作 '{}' 在 {} 次尝试后失败: {}", 
            operation_name, self.retry_config.max_attempts, final_error.to_log_string());
        
        Err(final_error)
    }
    
    /// 执行带有回退策略的操作
    pub async fn execute_with_fallback<F, Fut, FB, FutB, T>(
        &self, 
        primary_operation_name: &str,
        primary_operation: F,
        fallback_operation_name: &str,
        fallback_operation: FB
    ) -> PoolResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, anyhow::Error>>,
        FB: Fn() -> FutB,
        FutB: std::future::Future<Output = Result<T, anyhow::Error>>,
    {
        // 首先尝试主操作
        match self.enhanced_retry_async_operation(primary_operation_name, primary_operation).await {
            Ok(result) => Ok(result),
            Err(e) => {
                // 主操作失败，记录错误并尝试回退操作
                warn!("主操作 '{}' 失败，尝试回退操作 '{}': {}", 
                    primary_operation_name, fallback_operation_name, e.to_log_string());
                
                // 执行回退操作
                match self.enhanced_retry_async_operation(fallback_operation_name, fallback_operation).await {
                    Ok(result) => {
                        info!("回退操作 '{}' 成功", fallback_operation_name);
                        Ok(result)
                    },
                    Err(fallback_err) => {
                        error!("回退操作 '{}' 也失败: {}", fallback_operation_name, fallback_err.to_log_string());
                        // 返回回退操作的错误
                        Err(fallback_err)
                    }
                }
            }
        }
    }
    
    /// 获取最新的网络区块高度
    pub async fn get_latest_network_height(&self) -> PoolResult<u64> {
        // 首先检查环境变量，如果设置了MOCK_NETWORK_BLOCK_HEIGHT，则使用它（用于测试）
        if let Ok(height_str) = std::env::var("MOCK_NETWORK_BLOCK_HEIGHT") {
            if let Ok(height) = height_str.parse::<u64>() {
                info!("使用环境变量中设置的模拟网络区块高度: {}", height);
                return Ok(height);
            }
        }
        
        info!("尝试获取最新网络区块高度...");
        
        // 使用主要方法获取区块高度
        let result = self.execute_with_fallback(
            "从nockblocks.com获取区块高度",
            || self.fetch_height_from_nockblocks(),
            "使用备用方法估算区块高度",
            || self.estimate_network_height()
        ).await;
        
        match &result {
            Ok(height) => info!("成功获取网络区块高度: {}", height),
            Err(e) => warn!("获取网络区块高度失败: {}", e.to_log_string()),
        }
        
        result
    }
    
    /// 从nockblocks.com获取区块高度
    async fn fetch_height_from_nockblocks(&self) -> Result<u64, anyhow::Error> {
        info!("从nockblocks.com获取最新区块高度...");
        
        // 创建HTTP客户端
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?;
            
        // 获取网页内容
        let response = client.get("https://nockblocks.com/blocks")
            .header("User-Agent", "Mozilla/5.0 Nockchain-Pool-Server/0.1.0")
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                PoolServerError::NetworkError(format!("HTTP请求失败，状态码: {}", response.status()))
            ));
        }
        
        let html_content = response.text().await?;
        
        // 直接寻找"Latest Block"或"latest block"标记
        let latest_block_regex = Regex::new(r"[Ll]atest [Bb]lock.*?(\d+)").unwrap();
        if let Some(captures) = latest_block_regex.captures(&html_content) {
            if let Some(height_match) = captures.get(1) {
                if let Ok(height) = height_match.as_str().parse::<u64>() {
                    info!("成功提取最新区块高度: {}", height);
                    return Ok(height);
                } else {
                    return Err(anyhow::anyhow!(
                        PoolServerError::DataError("无法将提取的区块高度解析为数字".to_string())
                    ));
                }
            } else {
                return Err(anyhow::anyhow!(
                    PoolServerError::DataError("未能提取区块高度数字".to_string())
                ));
            }
        } else {
            return Err(anyhow::anyhow!(
                PoolServerError::DataError("未找到区块高度信息".to_string())
            ));
        }
    }
    
    /// 估算网络区块高度
    async fn estimate_network_height(&self) -> Result<u64, anyhow::Error> {
        warn!("使用备用方法估算网络区块高度");
        
        let current_height = if let Some(status_monitor) = &self.status_monitor {
            status_monitor.get_statistics().await.current_block_height
        } else {
            0
        };
        
        let estimated_height = current_height + 10; // 估算网络高度比本地高10个区块
        info!("使用估算的网络区块高度: {}", estimated_height);
        
        Ok(estimated_height)
    }
    
    /// 检查网络连接状态
    pub async fn check_network_connectivity(&self, endpoints: &[String]) -> PoolResult<bool> {
        info!("检查网络连接状态...");
        
        for endpoint in endpoints {
            match self.ping_endpoint(endpoint).await {
                Ok(true) => {
                    info!("成功连接到节点: {}", endpoint);
                    return Ok(true);
                },
                Ok(false) => {
                    warn!("无法连接到节点: {}", endpoint);
                },
                Err(e) => {
                    warn!("连接到节点 {} 时出错: {}", endpoint, e.to_log_string());
                }
            }
        }
        
        warn!("无法连接到任何节点");
        Err(PoolServerError::NetworkError("无法连接到任何节点".to_string()))
    }
    
    /// 测试与指定节点的连接
    async fn ping_endpoint(&self, endpoint: &str) -> PoolResult<bool> {
        // 简单的连接测试，可以根据实际协议进行调整
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| PoolServerError::NetworkError(format!("创建HTTP客户端失败: {}", e)))?;
            
        match client.head(endpoint).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(e) => {
                if e.is_timeout() {
                    Err(PoolServerError::NetworkError(format!("连接超时: {}", e)))
                } else if e.is_connect() {
                    Err(PoolServerError::NetworkError(format!("连接失败: {}", e)))
                } else {
                    Err(PoolServerError::NetworkError(format!("请求失败: {}", e)))
                }
            }
        }
    }

    /// 检查网络连接健康状态
    pub async fn check_network_health(&self) -> PoolResult<NetworkHealthStatus> {
        info!("检查网络连接健康状态...");
        
        // 测试与nockblocks.com的连接
        let nockblocks_status = self.ping_endpoint("https://nockblocks.com").await;
        
        // 测试与主网节点的连接
        let mainnet_status = self.ping_endpoint("https://p2p-mainnet.urbit.org").await;
        
        // 综合评估网络健康状态
        let health_status = match (nockblocks_status.is_ok(), mainnet_status.is_ok()) {
            (true, true) => NetworkHealthStatus::Healthy,
            (true, false) => NetworkHealthStatus::PartiallyHealthy,
            (false, true) => NetworkHealthStatus::PartiallyHealthy,
            (false, false) => NetworkHealthStatus::Unhealthy,
        };
        
        info!("网络健康状态: {}", health_status);
        
        Ok(health_status)
    }
}

/// 网络健康状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkHealthStatus {
    /// 健康：所有关键服务可访问
    Healthy,
    /// 部分健康：部分关键服务可访问
    PartiallyHealthy,
    /// 不健康：所有关键服务不可访问
    Unhealthy,
}

impl std::fmt::Display for NetworkHealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "健康"),
            Self::PartiallyHealthy => write!(f, "部分健康"),
            Self::Unhealthy => write!(f, "不健康"),
        }
    }
}

// 添加用于测试的函数
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PoolServerError;
    use tokio::time::timeout;

    // 测试网络重试机制
    #[tokio::test]
    async fn test_network_retry_mechanism() {
        // 创建网络管理器
        let network_manager = NetworkManager::new(RetryConfig {
            max_attempts: 2,
            initial_timeout_ms: 100,
            retry_interval_ms: 50,
            use_exponential_backoff: true,
        });
        
        // 测试成功的情况
        let success_result = network_manager.enhanced_retry_async_operation(
            "test_success",
            || async { Ok::<_, anyhow::Error>(42) },
        ).await;
        
        assert!(success_result.is_ok());
        assert_eq!(success_result.unwrap(), 42);
        
        // 测试失败后重试成功的情况
        let mut attempt = 0;
        let retry_success_result = network_manager.enhanced_retry_async_operation(
            "test_retry_success",
            || async {
                attempt += 1;
                if attempt == 1 {
                    Err(anyhow::anyhow!(PoolServerError::NetworkError("测试错误".to_string())))
                } else {
                    Ok(42)
                }
            },
        ).await;
        
        assert!(retry_success_result.is_ok());
        assert_eq!(retry_success_result.unwrap(), 42);
        
        // 测试超时情况
        let timeout_result = timeout(
            std::time::Duration::from_millis(50),
            network_manager.enhanced_retry_async_operation(
                "test_timeout",
                || async {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    Ok::<_, anyhow::Error>(42)
                },
            )
        ).await;
        
        assert!(timeout_result.is_err()); // 应该超时
    }
}