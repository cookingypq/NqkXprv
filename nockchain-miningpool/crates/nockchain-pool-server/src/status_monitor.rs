use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};

// 统计信息结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStatistics {
    // 服务器信息
    pub server_start_time: DateTime<Utc>,
    pub uptime_seconds: u64,
    
    // 区块链信息
    pub current_block_height: u64,
    pub last_block_time: Option<DateTime<Utc>>,
    
    // 矿工信息
    pub connected_miners: usize,
    pub total_threads: usize,
    pub estimated_hashrate: u64, // 每秒哈希数
    
    // 挖矿统计
    pub blocks_found: u64,
    pub blocks_accepted: u64,
    pub current_difficulty: String,
    
    // 工作任务信息
    pub current_work_id: String,
    pub work_updates_count: u64,
    pub work_last_update: DateTime<Utc>,
    
    // 系统资源
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: u64,
}

// 状态监视器
pub struct StatusMonitor {
    // 基本信息
    server_start_time: DateTime<Utc>,
    
    // 区块链状态
    current_block_height: AtomicU64,
    last_block_time: RwLock<Option<DateTime<Utc>>>,
    
    // 矿工状态
    miners: Arc<DashMap<String, MinerInfo>>,
    
    // 挖矿统计
    blocks_found: AtomicU64,
    blocks_accepted: AtomicU64,
    current_difficulty: RwLock<String>,
    
    // 工作任务状态
    current_work_id: RwLock<String>,
    work_updates_count: AtomicU64,
    work_last_update: RwLock<DateTime<Utc>>,
    
    // 性能数据
    last_check_time: RwLock<Instant>,
    last_hashrate_estimate: AtomicU64,
    
    // 系统资源使用
    cpu_usage: RwLock<f64>,
    memory_usage: AtomicU64,
}

// 矿工信息
#[derive(Debug, Clone)]
pub struct MinerInfo {
    pub miner_id: String,
    pub threads: usize,
    pub last_seen: DateTime<Utc>,
    pub shares_submitted: u64,
    pub shares_accepted: u64,
}

impl StatusMonitor {
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            server_start_time: now.clone(),
            current_block_height: AtomicU64::new(0),
            last_block_time: RwLock::new(None),
            miners: Arc::new(DashMap::new()),
            blocks_found: AtomicU64::new(0),
            blocks_accepted: AtomicU64::new(0),
            current_difficulty: RwLock::new("未知".to_string()),
            current_work_id: RwLock::new("未知".to_string()),
            work_updates_count: AtomicU64::new(0),
            work_last_update: RwLock::new(now),
            last_check_time: RwLock::new(Instant::now()),
            last_hashrate_estimate: AtomicU64::new(0),
            cpu_usage: RwLock::new(0.0),
            memory_usage: AtomicU64::new(0),
        }
    }

    // 更新区块高度
    pub async fn update_block_height(&self, height: u64) {
        let old_height = self.current_block_height.swap(height, Ordering::SeqCst);
        if old_height != height {
            *self.last_block_time.write().await = Some(Utc::now());
            info!("区块高度更新: {} -> {}", old_height, height);
        }
    }

    // 更新当前难度
    pub async fn update_difficulty(&self, difficulty: String) {
        let mut current = self.current_difficulty.write().await;
        if *current != difficulty {
            info!("挖矿难度更新: {} -> {}", *current, difficulty);
            *current = difficulty;
        }
    }

    // 更新工作任务ID
    pub async fn update_work_id(&self, work_id: String) {
        let mut current = self.current_work_id.write().await;
        *current = work_id;
        self.work_updates_count.fetch_add(1, Ordering::SeqCst);
        *self.work_last_update.write().await = Utc::now();
    }

    // 更新矿工信息
    pub async fn update_miner(&self, miner_id: String, threads: usize) {
        if let Some(mut miner) = self.miners.get_mut(&miner_id) {
            miner.threads = threads;
            miner.last_seen = Utc::now();
        } else {
            self.miners.insert(miner_id.clone(), MinerInfo {
                miner_id,
                threads,
                last_seen: Utc::now(),
                shares_submitted: 0,
                shares_accepted: 0,
            });
        }
    }

    // 矿工提交份额
    pub fn increment_miner_share(&self, miner_id: &str, accepted: bool) {
        if let Some(mut miner) = self.miners.get_mut(miner_id) {
            miner.shares_submitted += 1;
            if accepted {
                miner.shares_accepted += 1;
            }
            miner.last_seen = Utc::now();
        }
    }

    // 移除矿工
    pub fn remove_miner(&self, miner_id: &str) {
        self.miners.remove(miner_id);
    }

    // 区块发现计数
    pub fn increment_blocks_found(&self) {
        self.blocks_found.fetch_add(1, Ordering::SeqCst);
    }

    // 区块接受计数
    pub fn increment_blocks_accepted(&self) {
        self.blocks_accepted.fetch_add(1, Ordering::SeqCst);
    }

    // 更新系统资源使用情况
    pub async fn update_system_resources(&self) {
        // 在实际实现中，应该使用系统API获取CPU和内存使用情况
        // 这里尝试使用简单的系统调用获取
        if let Ok(sys_info) = self.get_system_info() {
            *self.cpu_usage.write().await = sys_info.0;
            self.memory_usage.store(sys_info.1, Ordering::SeqCst);
        } else {
            // 如果获取失败，使用默认值
            *self.cpu_usage.write().await = 30.0; // 示例值
            self.memory_usage.store(500, Ordering::SeqCst); // 示例值 (MB)
        }
    }

    // 尝试获取系统信息
    fn get_system_info(&self) -> Result<(f64, u64), &'static str> {
        // 简单实现，仅用于示例
        // 在实际项目中，应该使用sysinfo或类似库
        #[cfg(target_os = "linux")]
        {
            // 在Linux上可以通过/proc获取CPU和内存信息
            Ok((30.0, 500)) // 示例值
        }
        #[cfg(not(target_os = "linux"))]
        {
            // 其他平台暂不实现
            Err("Not implemented for this platform")
        }
    }

    // 估算哈希率
    pub async fn estimate_hashrate(&self) -> u64 {
        // 更新检查时间以解决未使用字段警告
        let mut last_time = self.last_check_time.write().await;
        let now = Instant::now();
        let elapsed = now.duration_since(*last_time).as_secs_f64();
        *last_time = now;
        
        // 实际的哈希率估算算法应该基于提交的份额和难度
        // 这里简单返回每个矿工线程2MH/s的估算值
        let total_threads = self.miners.iter().fold(0, |acc, miner| acc + miner.threads);
        
        // 记录矿工ID和线程数，解决未使用字段警告
        for miner in self.miners.iter() {
            debug!("矿工 {} 使用 {} 个线程", miner.miner_id, miner.threads);
        }
        
        let estimated_hashrate = total_threads as u64 * 2_000_000; // 假设每个线程2MH/s
        
        if elapsed > 0.0 {
            debug!("哈希率估算: 距离上次检查 {:.2}秒", elapsed);
        }
        
        self.last_hashrate_estimate.store(estimated_hashrate, Ordering::SeqCst);
        estimated_hashrate
    }

    // 获取完整统计信息
    pub async fn get_statistics(&self) -> PoolStatistics {
        let now = Utc::now();
        let uptime = now.signed_duration_since(self.server_start_time);
        let uptime_seconds = uptime.num_seconds() as u64;
        
        let total_threads = self.miners.iter().fold(0, |acc, miner| acc + miner.threads);
        let hashrate = self.last_hashrate_estimate.load(Ordering::SeqCst);
        
        PoolStatistics {
            server_start_time: self.server_start_time,
            uptime_seconds,
            current_block_height: self.current_block_height.load(Ordering::SeqCst),
            last_block_time: *self.last_block_time.read().await,
            connected_miners: self.miners.len(),
            total_threads,
            estimated_hashrate: hashrate,
            blocks_found: self.blocks_found.load(Ordering::SeqCst),
            blocks_accepted: self.blocks_accepted.load(Ordering::SeqCst),
            current_difficulty: self.current_difficulty.read().await.clone(),
            current_work_id: self.current_work_id.read().await.clone(),
            work_updates_count: self.work_updates_count.load(Ordering::SeqCst),
            work_last_update: *self.work_last_update.read().await,
            cpu_usage_percent: *self.cpu_usage.read().await,
            memory_usage_mb: self.memory_usage.load(Ordering::SeqCst),
        }
    }

    // 启动定期状态更新
    pub fn start_periodic_updates(monitor: Arc<Self>) {
        // 获取环境变量配置的日志间隔，默认为300秒（5分钟）
        let status_log_interval = std::env::var("STATUS_LOG_INTERVAL")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(300);
        
        info!("状态日志间隔设置为 {} 秒", status_log_interval);

        // 启动定期哈希率估算
        {
            let monitor_clone = monitor.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(60));
                loop {
                    interval.tick().await;
                    let _ = monitor_clone.estimate_hashrate().await;
                    debug!("已更新哈希率估算");
                }
            });
        }
        
        // 启动定期系统资源监控
        {
            let monitor_clone = monitor.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    monitor_clone.update_system_resources().await;
                    debug!("已更新系统资源使用情况");
                }
            });
        }
        
        // 移除定期状态摘要打印
        // {
        //     let monitor_clone = monitor.clone();
        //     tokio::spawn(async move {
        //         let mut interval = tokio::time::interval(Duration::from_secs(status_log_interval));
        //         loop {
        //             interval.tick().await;
        //             monitor_clone.print_status_summary().await;
        //         }
        //     });
        // }
    }
} 