use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tonic::transport::Channel;
use tonic::{Request, Streaming};
use tracing::{error, info, warn};
use std::time::Instant;
use blake3;

// 导入生成的protobuf代码 | Import generated protobuf code
#[cfg(feature = "pool_client")]
pub mod pool {
    tonic::include_proto!("pool");
}

#[cfg(feature = "pool_client")]
use pool::{
    mining_pool_client::MiningPoolClient,
    MinerStatus, WorkOrder, WorkResult,
};

/// 矿池客户端配置 | Pool client configuration
pub struct PoolClientConfig {
    /// 矿池服务器地址 | Pool server address
    pub server_address: String,
    /// 矿工线程数 | Miner thread count
    pub threads: u32,
    /// 矿工ID | Miner ID
    pub miner_id: String,
}

/// 矿池客户端 | Pool client
#[cfg(feature = "pool_client")]
pub struct PoolClient {
    config: PoolClientConfig,
    client: MiningPoolClient<Channel>,
}

#[cfg(feature = "pool_client")]
impl PoolClient {
    /// 创建一个新的矿池客户端 | Create a new pool client
    pub async fn new(config: PoolClientConfig) -> Result<Self> {
        let client = MiningPoolClient::connect(format!("http://{}", config.server_address))
            .await
            .context("连接到矿池服务器失败 | Failed to connect to pool server")?;

        Ok(Self { config, client })
    }

    /// 启动矿池客户端 | Start the pool client
    pub async fn run(mut self) -> Result<()> {
        info!(
            "连接到矿池服务器: {}, 使用 {} 个线程 | Connecting to pool server: {}, using {} threads",
            self.config.server_address, self.config.threads, self.config.server_address, self.config.threads
        );

        // 创建订阅请求 | Create subscription request
        let (status_tx, mut status_rx) = mpsc::channel(10);

        // 发送初始状态 | Send initial status
        let initial_status = MinerStatus {
            miner_id: self.config.miner_id.clone(),
            threads: self.config.threads,
        };

        status_tx
            .send(initial_status)
            .await
            .context("发送初始状态失败 | Failed to send initial status")?;

        // 创建订阅流 | Create subscription stream
        let outbound_stream = async_stream::stream! {
            while let Some(status) = status_rx.recv().await {
                yield status;
            }
        };

        // 发起订阅 | Initiate subscription
        let response = self.client
            .subscribe(Request::new(outbound_stream))
            .await
            .context("订阅矿池工作任务失败 | Failed to subscribe to pool work orders")?;

        // 获取工作订单流 | Get work order stream
        let mut work_stream = response.into_inner();

        // 处理工作订单 | Process work orders
        self.process_work_stream(&mut work_stream).await?;

        Ok(())
    }

    /// 处理工作订单流 | Process work order stream
    async fn process_work_stream(&mut self, work_stream: &mut Streaming<WorkOrder>) -> Result<()> {
        // 共享的当前工作订单 | Shared current work order
        let current_work = Arc::new(Mutex::new(None));

        while let Some(work_order) = work_stream
            .message()
            .await
            .context("接收工作任务失败 | Failed to receive work order")?
        {
            info!(
                "收到新工作任务: {} | Received new work order: {}",
                work_order.work_id, work_order.work_id
            );

            // 更新当前工作订单 | Update current work order
            let mut current_work_lock = current_work.lock().await;
            *current_work_lock = Some(work_order.clone());
            drop(current_work_lock);

            // 创建多个计算线程 | Create multiple computation threads
            let client_clone = self.client.clone();
            let miner_id = self.config.miner_id.clone();
            let work_order_clone = work_order.clone();
            let current_work_clone = current_work.clone();

            let mut handles = vec![];

            // 为每个线程创建一个任务 | Create a task for each thread
            for thread_id in 0..self.config.threads {
                let client = client_clone.clone();
                let miner_id = miner_id.clone();
                let work_order = work_order_clone.clone();
                let current_work = current_work_clone.clone();

                let handle = tokio::spawn(async move {
                    // 在工作线程中执行计算 | Perform computation in worker thread
                    Self::compute_pow(
                        client,
                        thread_id,
                        miner_id,
                        work_order,
                        current_work,
                    )
                    .await
                });

                handles.push(handle);
            }

            // 等待所有线程完成 | Wait for all threads to complete
            for handle in handles {
                if let Err(e) = handle.await {
                    error!(
                        "工作线程发生错误: {} | Work thread encountered an error: {}", 
                        e, e
                    );
                }
            }
        }

        warn!("工作流结束，与服务器的连接已断开 | Work stream ended, connection to server has been lost");
        Ok(())
    }

    /// 执行PoW计算 | Perform PoW computation
    async fn compute_pow(
        mut client: MiningPoolClient<Channel>,
        thread_id: u32,
        miner_id: String,
        work_order: WorkOrder,
        current_work: Arc<Mutex<Option<WorkOrder>>>,
    ) -> Result<()> {
        let work_id = work_order.work_id.clone();
        let thread_miner_id = format!("{}-{}", miner_id, thread_id);
        
        // 从工作订单中提取必要信息
        // Extract necessary information from work order
        let parent_hash = &work_order.parent_hash;
        let merkle_root = &work_order.merkle_root;
        let timestamp = work_order.timestamp;
        let difficulty_target = &work_order.difficulty_target;
        
        info!("线程 {} 开始计算 | Thread {} starting computation", thread_id, thread_id);
        
        // 不使用equix Solver，因为库可能不兼容
        // 改为直接使用blake3进行哈希计算
        
        // 随机初始nonce
        // Random initial nonce
        let mut nonce = rand::random::<[u8; 32]>();
        
        // 开始计算时间
        // Start computation time
        let start_time = Instant::now();
        let mut hash_count = 0u64;
        
        loop {
            // 每1000次哈希计算检查一次当前工作是否还有效
            // Check if current work is still valid every 1000 hash calculations
            if hash_count % 1000 == 0 {
                // 检查当前工作是否还有效 | Check if current work is still valid
                let current = {
                    let lock = current_work.lock().await;
                    match &*lock {
                        Some(current) => current.work_id.clone(),
                        None => work_id.clone(),
                    }
                };

                if current != work_id {
                    // 工作任务已经变更，停止计算 | Work task has changed, stop computation
                    info!(
                        "工作任务已变更，线程 {} 停止计算 | Work task has changed, thread {} stops computation",
                        thread_id, thread_id
                    );
                    break;
                }
                
                // 每5秒输出一次哈希率
                // Output hash rate every 5 seconds
                if start_time.elapsed().as_secs() >= 5 && start_time.elapsed().as_secs() % 5 == 0 {
                    let hash_rate = hash_count as f64 / start_time.elapsed().as_secs_f64();
                    info!(
                        "线程 {} 哈希率: {:.2} H/s | Thread {} hash rate: {:.2} H/s",
                        thread_id, hash_rate, thread_id, hash_rate
                    );
                }
            }
            
            // 构建区块头 (parent_hash + merkle_root + timestamp + nonce)
            // Build block header (parent_hash + merkle_root + timestamp + nonce)
            let mut block_data = Vec::with_capacity(parent_hash.len() + merkle_root.len() + 8 + 32);
            block_data.extend_from_slice(parent_hash);
            block_data.extend_from_slice(merkle_root);
            block_data.extend_from_slice(&timestamp.to_le_bytes());
            block_data.extend_from_slice(&nonce);
            
            // 计算区块哈希
            // Calculate block hash
            let hash = blake3::hash(&block_data);
            hash_count += 1;
            
            // 比较哈希与目标难度
            // Compare hash with target difficulty
            if compare_hash_with_target(hash.as_bytes(), difficulty_target) {
                // 找到解决方案，提交工作结果 | Found a solution, submit work result
                info!(
                    "线程 {} 找到解决方案！提交工作... | Thread {} found a solution! Submitting work...",
                    thread_id, thread_id
                );

                let result = WorkResult {
                    work_id: work_id.clone(),
                    nonce: nonce.to_vec(), // 实际找到的nonce | Actually found nonce
                    miner_id: thread_miner_id.clone(),
                };

                match client.submit_work(Request::new(result)).await {
                    Ok(response) => {
                        let ack = response.into_inner();
                        if ack.success {
                            info!(
                                "工作被池接受！ | Work accepted by pool!"
                            );
                        } else {
                            warn!(
                                "工作被池拒绝 | Work rejected by pool"
                            );
                        }
                        break;
                    }
                    Err(e) => {
                        error!(
                            "提交工作时发生错误: {} | Error occurred while submitting work: {}", 
                            e, e
                        );
                        break;
                    }
                }
            }
            
            // 增加nonce
            // Increase nonce
            for i in 0..32 {
                nonce[i] = nonce[i].wrapping_add(1);
                if nonce[i] != 0 {
                    break;
                }
            }
        }

        Ok(())
    }
}

// 比较哈希与目标难度
// Compare hash with target difficulty
fn compare_hash_with_target(hash: &[u8], target: &[u8]) -> bool {
    // 难度比较算法：哈希值必须小于目标值
    // Difficulty comparison algorithm: hash must be less than target
    for i in 0..hash.len().min(target.len()) {
        if hash[i] < target[i] {
            return true;
        }
        if hash[i] > target[i] {
            return false;
        }
    }
    // 如果所有字节都相等，则哈希等于目标，通常不满足POW
    // If all bytes are equal, hash equals target, usually doesn't satisfy POW
    false
} 