use anyhow::Result;
use tokio::sync::mpsc;
use tonic::transport::Channel;
use tonic::Request;
use std::time::Duration;
use tracing::{info, warn, error};
use tokio::time::timeout;
use std::sync::atomic::{AtomicBool, Ordering};

// 包含由 build.rs 在 OUT_DIR 中生成的代码
// 这会引入 pool 模块
pub mod pool {
    tonic::include_proto!("pool");
}

use pool::{
    mining_pool_client::MiningPoolClient,
    MinerStatus, WorkResult,
};

/// PoW 计算的核心逻辑
/// 返回找到的 nonce，如果计算被中断或未找到，则返回 None。
fn mine(parent_hash: &[u8], merkle_root: &[u8], difficulty_target: &[u8], threads: u32) -> Option<Vec<u8>> {
    let (nonce_sender, mut nonce_receiver) = mpsc::channel(1);
    let found_nonce = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    for i in 0..threads {
        let parent_hash = parent_hash.to_vec();
        let merkle_root = merkle_root.to_vec();
        let difficulty_target = difficulty_target.to_vec();
        let sender = nonce_sender.clone();
        let found = found_nonce.clone();

        tokio::spawn(async move {
            let mut nonce = [0u8; 32];
            // 为每个线程设置不同的起始nonce，避免重复工作
            nonce[0] = i as u8;

            while !found.load(std::sync::atomic::Ordering::SeqCst) {
                let mut hasher = blake3::Hasher::new();
                hasher.update(&parent_hash);
                hasher.update(&merkle_root);
                hasher.update(&nonce);

                let hash = hasher.finalize();

                if (hash.as_bytes() as &[u8]) < &difficulty_target[..] {
                    if found.compare_exchange(false, true, std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst).is_ok() {
                        let _ = sender.send(nonce.to_vec()).await;
                    }
                    return;
                }
                
                // 简单的、包装安全的nonce递增
                for byte in nonce.iter_mut() {
                    *byte = byte.wrapping_add(1);
                    if *byte != 0 {
                        break;
                    }
                }
            }
        });
    }

    // 等待第一个有效的nonce
    nonce_receiver.blocking_recv()
}

pub struct PoolClientConfig {
    pub server_address: String,
    pub threads: u32,
    pub miner_id: String,
    pub connection_retry_attempts: u32,  // 连接重试次数
    pub connection_retry_delay_ms: u64,  // 重试延迟（毫秒）
    pub connection_timeout_ms: u64,      // 连接超时（毫秒）
    pub request_timeout_ms: u64,         // 请求超时（毫秒）
    pub backup_servers: Vec<String>,     // 备用服务器列表
}

impl Default for PoolClientConfig {
    fn default() -> Self {
        Self {
            server_address: "http://localhost:7777".to_string(),
            threads: 1,
            miner_id: "unknown".to_string(),
            connection_retry_attempts: 5,
            connection_retry_delay_ms: 1000,
            connection_timeout_ms: 5000,
            request_timeout_ms: 10000,
            backup_servers: vec![],
        }
    }
}

pub struct PoolClient {
    config: PoolClientConfig,
    client: MiningPoolClient<Channel>,
    is_connected: AtomicBool,
    active_server: String,
}

impl PoolClient {
    pub async fn new(config: PoolClientConfig) -> Result<Self> {
        let (client, active_server) = Self::connect_with_retry(&config).await?;
        
        Ok(Self { 
            config, 
            client, 
            is_connected: AtomicBool::new(true),
            active_server,
        })
    }
    
    // 连接服务器，带有重试机制
    async fn connect_with_retry(config: &PoolClientConfig) -> Result<(MiningPoolClient<Channel>, String)> {
        let servers = std::iter::once(config.server_address.clone())
            .chain(config.backup_servers.clone())
            .collect::<Vec<_>>();
            
        let mut last_error: Option<anyhow::Error> = None;
        
        for server in &servers {
            for attempt in 1..=config.connection_retry_attempts {
                info!("尝试连接到矿池服务器 {} (尝试 {}/{})", server, attempt, config.connection_retry_attempts);
                
                match timeout(
                    Duration::from_millis(config.connection_timeout_ms),
                    MiningPoolClient::connect(server.clone())
                ).await {
                    Ok(Ok(client)) => {
                        info!("成功连接到矿池服务器: {}", server);
                        return Ok((client, server.clone()));
                    },
                    Ok(Err(e)) => {
                        warn!("连接到矿池服务器 {} 失败: {}", server, e);
                        last_error = Some(anyhow::anyhow!("连接失败: {}", e));
                    },
                    Err(_) => {
                        warn!("连接到矿池服务器 {} 超时", server);
                        last_error = Some(anyhow::anyhow!("连接超时"));
                    }
                }
                
                if attempt < config.connection_retry_attempts {
                    let delay = config.connection_retry_delay_ms * attempt as u64;
                    info!("等待 {}ms 后重试连接...", delay);
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
            }
        }
        
        // 如果所有服务器都连接失败，返回最后一个错误
        Err(last_error.unwrap_or_else(|| {
            anyhow::anyhow!("无法连接到任何矿池服务器")
        }))
    }
    
    // 重新连接到服务器
    async fn reconnect(&mut self) -> Result<()> {
        self.is_connected.store(false, Ordering::SeqCst);
        
        info!("尝试重新连接到矿池服务器...");
        let (new_client, active_server) = Self::connect_with_retry(&self.config).await?;
        
        self.client = new_client;
        self.active_server = active_server;
        self.is_connected.store(true, Ordering::SeqCst);
        
        info!("成功重新连接到矿池服务器: {}", self.active_server);
        Ok(())
    }

    pub async fn run(mut self) -> Result<()> {
        // 创建一个初始的MinerStatus流，只发送一次
        let initial_status_template = MinerStatus {
            miner_id: self.config.miner_id.clone(),
            threads: self.config.threads,
        };
        
        // 订阅服务器，带有重试机制
        let mut retry_count = 0;
        let max_retries = self.config.connection_retry_attempts;
        let mut stream = loop {
            // 每次循环创建一个新的stream，使用模板克隆一个新的status
            // 每次循环都克隆一次miner_id，避免移动
            let miner_id_clone = initial_status_template.miner_id.clone();
            let threads = initial_status_template.threads;
            let request_stream = async_stream::stream! {
                yield MinerStatus {
                    miner_id: miner_id_clone,
                    threads: threads,
                };
                // 在此之后流保持打开，但不再发送任何内容
            };
            
            match timeout(
                Duration::from_millis(self.config.request_timeout_ms),
                self.client.subscribe(Request::new(request_stream))
            ).await {
                Ok(Ok(response)) => {
                    info!("成功订阅矿池服务器");
                    break response.into_inner();
                },
                Ok(Err(e)) => {
                    retry_count += 1;
                    warn!("订阅矿池服务器失败 (尝试 {}/{}): {}", retry_count, max_retries, e);
                    
                    if retry_count >= max_retries {
                        return Err(anyhow::anyhow!("订阅矿池服务器失败，已达到最大重试次数: {}", e));
                    }
                    
                    // 尝试重新连接
                    if let Err(reconnect_err) = self.reconnect().await {
                        error!("重新连接失败: {}", reconnect_err);
                        return Err(reconnect_err);
                    }
                    
                    // 重试前等待
                    let delay = self.config.connection_retry_delay_ms * retry_count as u64;
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                },
                Err(_) => {
                    retry_count += 1;
                    warn!("订阅矿池服务器请求超时 (尝试 {}/{})", retry_count, max_retries);
                    
                    if retry_count >= max_retries {
                        return Err(anyhow::anyhow!("订阅矿池服务器请求超时，已达到最大重试次数"));
                    }
                    
                    // 尝试重新连接
                    if let Err(reconnect_err) = self.reconnect().await {
                        error!("重新连接失败: {}", reconnect_err);
                        return Err(reconnect_err);
                    }
                    
                    // 重试前等待
                    let delay = self.config.connection_retry_delay_ms * retry_count as u64;
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
            }
        };

        info!("已成功订阅矿池服务器，等待工作任务...");

        // 循环接收工作任务
        while let Some(work_order_result) = Self::receive_message_with_timeout(&mut stream, self.config.request_timeout_ms).await {
            match work_order_result {
                Ok(work_order) => {
                    info!("接收到新工作任务: ID {}", work_order.work_id);
                    
                    // 在一个新任务中处理挖矿，以避免阻塞主循环
                    let threads = self.config.threads;
                    let miner_id = self.config.miner_id.clone(); // 克隆一次，避免多次移动
                    let mut client_clone = self.client.clone();
                    let request_timeout_ms = self.config.request_timeout_ms;

                    tokio::spawn(async move {
                        if let Some(nonce) = mine(
                            &work_order.parent_hash,
                            &work_order.merkle_root,
                            &work_order.difficulty_target,
                            threads,
                        ) {
                            info!("找到有效 Nonce: {}...!", hex::encode(&nonce[0..8]));
                            let work_result = WorkResult {
                                work_id: work_order.work_id.clone(),
                                nonce,
                                miner_id,
                            };

                            // 提交工作结果，带有超时
                            match timeout(
                                Duration::from_millis(request_timeout_ms),
                                client_clone.submit_work(Request::new(work_result.clone()))
                            ).await {
                                Ok(Ok(_)) => {
                                    info!("工作 {} 已成功提交。", work_order.work_id);
                                },
                                Ok(Err(e)) => {
                                    error!("提交工作时出错: {}", e);
                                    
                                    // 可以在这里添加重试逻辑
                                    Self::retry_submit_work(&mut client_clone, work_result, 3, request_timeout_ms).await;
                                },
                                Err(_) => {
                                    error!("提交工作请求超时");
                                    
                                    // 可以在这里添加重试逻辑
                                    Self::retry_submit_work(&mut client_clone, work_result, 3, request_timeout_ms).await;
                                }
                            }
                        }
                    });
                },
                Err(e) => {
                    error!("接收工作任务时出错: {}", e);
                    
                    // 尝试重新连接
                    if let Err(reconnect_err) = self.reconnect().await {
                        error!("重新连接失败: {}", reconnect_err);
                        return Err(reconnect_err);
                    }
                    
                    // 重新订阅
                    // 克隆一次miner_id，避免移动
                    let miner_id_clone = self.config.miner_id.clone();
                    let threads = self.config.threads;
                    let request_stream = async_stream::stream! {
                        yield MinerStatus {
                            miner_id: miner_id_clone,
                            threads: threads,
                        };
                    };
                    
                    match self.client.subscribe(Request::new(request_stream)).await {
                        Ok(response) => {
                            stream = response.into_inner();
                            info!("成功重新订阅矿池服务器");
                        },
                        Err(e) => {
                            error!("重新订阅失败: {}", e);
                            return Err(anyhow::anyhow!("重新订阅失败: {}", e));
                        }
                    }
                }
            }
        }

        Ok(())
    }
    
    // 带有超时的消息接收
    async fn receive_message_with_timeout<T>(
        stream: &mut tonic::Streaming<T>, 
        timeout_ms: u64
    ) -> Option<Result<T>> {
        match timeout(Duration::from_millis(timeout_ms), stream.message()).await {
            Ok(result) => match result {
                Ok(message) => message.map(Ok),
                Err(e) => Some(Err(anyhow::anyhow!("接收消息错误: {}", e))),
            },
            Err(_) => Some(Err(anyhow::anyhow!("接收消息超时"))),
        }
    }
    
    // 重试提交工作结果
    async fn retry_submit_work(
        client: &mut MiningPoolClient<Channel>,
        work_result: WorkResult,
        max_retries: u32,
        timeout_ms: u64
    ) {
        for attempt in 1..=max_retries {
            info!("重试提交工作结果 (尝试 {}/{})", attempt, max_retries);
            
            // 等待一段时间后重试
            tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
            
            match timeout(
                Duration::from_millis(timeout_ms),
                client.submit_work(Request::new(work_result.clone()))
            ).await {
                Ok(Ok(_)) => {
                    info!("工作 {} 在重试后成功提交", work_result.work_id);
                    return;
                },
                Ok(Err(e)) => {
                    warn!("重试提交工作时出错: {}", e);
                },
                Err(_) => {
                    warn!("重试提交工作请求超时");
                }
            }
        }
        
        error!("提交工作 {} 失败，已达到最大重试次数", work_result.work_id);
    }
} 