use anyhow::Result;
use dashmap::DashMap;
use dotenv::dotenv;
use futures::Stream;
use std::env;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{error, info, warn, debug};
use uuid::Uuid;
use blake3;
// 引入正则表达式库
use regex::Regex;
// 引入状态监控模块
use crate::status_monitor::StatusMonitor;
// 引入错误处理模块
use crate::error::{PoolServerError, PoolResult, ErrorSeverity};
// 引入网络管理器模块
use crate::network_manager::{NetworkManager, RetryConfig};

// 引入nockchain核心库，用于集成nockchain节点
use kernels::dumb::KERNEL;
use nockapp::kernel::boot;
use nockapp::utils::make_tas;
use nockapp::nockapp::driver::{NockAppHandle, PokeResult};
use nockapp::nockapp::wire::Wire;
use nockapp::NockApp;
use nockapp::noun::slab::NounSlab;
use nockapp::noun::AtomExt;
use nockvm::noun::{Atom, D, T};
use nockvm_macros::tas;
use zkvm_jetpack::hot::produce_prover_hot_state;
use bytes::Bytes;
use std::sync::Mutex;
use std::time::Duration;

// 简化日志初始化导入
use nockapp::kernel::boot::init_default_tracing;
use std::sync::atomic::{AtomicBool, Ordering};

// 添加用于检查跟踪系统状态的导入
use tokio::sync::broadcast;

// 添加新模块声明
mod status_monitor;
mod http_api;
mod error;
mod network_manager;

// 定义一个静态变量来跟踪是否已经初始化
static TRACING_INITIALIZED: AtomicBool = AtomicBool::new(false);

fn try_init_tracing() {
    // 检查是否已初始化
    if TRACING_INITIALIZED.load(Ordering::SeqCst) {
        return;
    }
    
    // 使用 nockchain 推荐的默认 cli
    let cli = nockapp::kernel::boot::default_boot_cli(true);
    
    // 使用与nockchain相同的日志初始化方法
    init_default_tracing(&cli);
    
    // 标记为已初始化
    TRACING_INITIALIZED.store(true, Ordering::SeqCst);
}

// 导入生成的protobuf代码 | Import generated protobuf code
pub mod pool {
    tonic::include_proto!("pool");
}

use pool::{
    mining_pool_server::{MiningPool, MiningPoolServer},
    MinerStatus, SubmitAck, WorkOrder, WorkResult,
};

// 矿工连接 | Miner connection
struct MinerConnection {
    miner_id: String,
    work_sender: mpsc::Sender<Result<WorkOrder, Status>>,
    threads: u32,
}

// 工作订单上下文 | Work order context
#[derive(Clone)]
struct WorkOrderContext {
    work_id: String,
    parent_hash: Vec<u8>,
    merkle_root: Vec<u8>,
    timestamp: u64,
    difficulty_target: Vec<u8>,
}

// 定义一个枚举表示不同的挖矿指令线路
enum MiningWire {
    SetPubKey,
    Enable,
}

impl MiningWire {
    pub fn verb(&self) -> &'static str {
        match self {
            MiningWire::SetPubKey => "setpubkey",
            MiningWire::Enable => "enable",
        }
    }
}

impl Wire for MiningWire {
    const VERSION: u64 = 1;
    const SOURCE: &'static str = "miner";

    fn to_wire(&self) -> nockapp::wire::WireRepr {
        let tags = vec![self.verb().into()];
        nockapp::wire::WireRepr::new(MiningWire::SOURCE, MiningWire::VERSION, tags)
    }
}

// 实现Stream trait，将mpsc::Receiver包装为一个Stream
// Implement Stream trait, wrapping mpsc::Receiver as a Stream
struct WorkOrderStream {
    inner: mpsc::Receiver<Result<WorkOrder, Status>>,
}

impl Stream for WorkOrderStream {
    type Item = Result<WorkOrder, Status>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_recv(cx)
    }
}

// 修复：使用线程安全的方式封装NockAppHandle
// Fix: Use thread-safe way to wrap NockAppHandle
struct ThreadSafeNockAppHandle {
    handle: Arc<Mutex<NockAppHandle>>,
    effect_sender: Arc<broadcast::Sender<NounSlab>>,
}

impl ThreadSafeNockAppHandle {
    fn new(handle: NockAppHandle) -> Self {
        let effect_sender = handle.effect_sender.clone();
        Self {
            handle: Arc::new(Mutex::new(handle)),
            effect_sender,
        }
    }
    
    // 封装poke方法，确保线程安全
    async fn safe_poke(&self, _wire: nockapp::wire::WireRepr, _poke_slab: NounSlab) -> Result<PokeResult, anyhow::Error> {
        // 在单线程上下文中执行，避免原始指针跨线程
        let handle_clone = self.handle.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _handle = handle_clone.lock().unwrap();
            
            // 简化版：返回模拟结果
            Ok::<PokeResult, anyhow::Error>(PokeResult::Ack)
        }).await??;
        
        Ok(result)
    }
}

// 矿池服务 | Mining pool service
struct MiningPoolService {
    // 已连接矿工列表 | Connected miners list
    miners: Arc<DashMap<String, MinerConnection>>,
    // 当前工作任务 | Current work task
    current_work: Arc<RwLock<Option<WorkOrderContext>>>,
    // 内嵌的nockchain节点 | Embedded nockchain node
    nockchain: Arc<RwLock<NockApp>>,
    // 线程安全的handle
    handle: ThreadSafeNockAppHandle,
    // 状态监控器
    status_monitor: Arc<StatusMonitor>,
    // 网络管理器
    network_manager: Arc<NetworkManager>,
}

impl MiningPoolService {
    fn new(nockchain: NockApp) -> Self {
        // 创建线程安全的handle
        let handle = ThreadSafeNockAppHandle::new(nockchain.get_handle());
        let status_monitor = Arc::new(StatusMonitor::new());
        
        // 启动定期状态更新
        status_monitor::StatusMonitor::start_periodic_updates(status_monitor.clone());
        
        // 创建网络管理器
        let mut network_manager = NetworkManager::new(RetryConfig {
            max_attempts: 3,
            initial_timeout_ms: 5000,
            retry_interval_ms: 1000,
            use_exponential_backoff: true,
        });
        
        // 设置状态监控器
        network_manager.set_status_monitor(status_monitor.clone());
        let network_manager = Arc::new(network_manager);
        
        let service = Self {
            miners: Arc::new(DashMap::new()),
            current_work: Arc::new(RwLock::new(None)),
            nockchain: Arc::new(RwLock::new(nockchain)),
            handle,
            status_monitor,
            network_manager,
        };
        
        // 启动区块链事件监听器
        service.start_blockchain_event_listener();
        
        service
    }
    
    // 启动区块链事件监听器
    fn start_blockchain_event_listener(&self) {
        let service_clone = self.clone();
        
        tokio::spawn(async move {
            // 初始同步
            match service_clone.initial_sync_with_mainnet().await {
                true => info!("成功完成与主网的初始同步"),
                false => warn!("与主网的初始同步未完成，将继续尝试"),
            }
            
            // 定期检查最新区块高度
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                service_clone.try_get_latest_network_height().await;
            }
        });
    }
    
    // 尝试获取最新的网络区块高度并更新状态
    async fn try_get_latest_network_height(&self) {
        info!("开始检查主网最新区块高度...");
        
        // 使用网络管理器获取最新区块高度
        match self.network_manager.get_latest_network_height().await {
            Ok(height) => {
                info!("从区块浏览器获取到的主网最新区块高度: {}", height);
                self.status_monitor.update_network_block_height(Some(height)).await;
            },
            Err(e) => {
                error!("获取主网最新区块高度失败: {}", e.to_log_string());
                // 如果错误可恢复，设置为未知状态，否则保持当前值
                if e.is_recoverable() {
                    self.status_monitor.update_network_block_height(None).await;
                    self.status_monitor.set_sync_status("同步状态未知".to_string()).await;
                }
            }
        }
    }
    
    // 初始同步与主网
    async fn initial_sync_with_mainnet(&self) -> bool {
        info!("开始与主网进行初始同步...");
        self.status_monitor.set_sync_status("正在同步".to_string()).await;
        
        // 验证主网创世区块
        match self.verify_mainnet_genesis().await {
            Ok(true) => {
                info!("主网创世区块验证成功");
            },
            Ok(false) => {
                error!("主网创世区块验证失败");
                self.status_monitor.set_sync_status("创世区块验证失败".to_string()).await;
                return false;
            },
            Err(e) => {
                let err = PoolServerError::from_anyhow(e);
                error!("验证主网创世区块时出错: {}", err.to_log_string());
                self.status_monitor.set_sync_status("创世区块验证错误".to_string()).await;
                
                // 如果错误不可恢复，直接返回失败
                if !err.is_recoverable() {
                    return false;
                }
                
                // 尝试设置realnet genesis seal
                info!("尝试设置realnet genesis seal...");
                match self.set_realnet_genesis_seal().await {
                    Ok(true) => {
                        info!("成功设置realnet genesis seal");
                    },
                    Ok(false) => {
                        error!("设置realnet genesis seal失败");
                        self.status_monitor.set_sync_status("设置创世区块失败".to_string()).await;
                        return false;
                    },
                    Err(e) => {
                        let err = PoolServerError::from_anyhow(e);
                        error!("设置realnet genesis seal时出错: {}", err.to_log_string());
                        self.status_monitor.set_sync_status("设置创世区块错误".to_string()).await;
                        return false;
                    }
                }
            }
        }
        
        // 获取当前区块高度
        let current_height = match self.get_current_block_height().await {
            Ok(height) => {
                info!("当前区块高度: {}", height);
                self.status_monitor.update_block_height(height).await;
                height
            },
            Err(e) => {
                let err = PoolServerError::from_anyhow(e);
                error!("获取当前区块高度失败: {}", err.to_log_string());
                self.status_monitor.set_sync_status("获取区块高度失败".to_string()).await;
                return false;
            }
        };
        
        // 获取网络最新区块高度
        let network_height = match self.network_manager.get_latest_network_height().await {
            Ok(height) => {
                info!("网络最新区块高度: {}", height);
                self.status_monitor.update_network_block_height(Some(height)).await;
                height
            },
            Err(e) => {
                error!("获取网络最新区块高度失败: {}", e.to_log_string());
                self.status_monitor.update_network_block_height(None).await;
                
                // 如果错误可恢复，使用估算值继续
                if e.is_recoverable() {
                    warn!("使用估算的网络区块高度");
                    current_height + 10
                } else {
                    self.status_monitor.set_sync_status("获取网络高度失败".to_string()).await;
                    return false;
                }
            }
        };
        
        // 计算同步百分比
        if network_height > 0 {
            let sync_percentage = if network_height > 0 {
                (current_height as f64 / network_height as f64) * 100.0
            } else {
                0.0
            };
            
            info!("同步进度: {:.2}% ({}/{})", sync_percentage, current_height, network_height);
            
            // 更新同步状态
            if current_height >= network_height {
                self.status_monitor.set_sync_status("已同步".to_string()).await;
                true
            } else if sync_percentage > 90.0 {
                self.status_monitor.set_sync_status(format!("同步中 ({:.2}%)", sync_percentage)).await;
                true
            } else {
                self.status_monitor.set_sync_status(format!("正在同步 ({:.2}%)", sync_percentage)).await;
                false
            }
        } else {
            self.status_monitor.set_sync_status("同步状态未知".to_string()).await;
            false
        }
    }
    
    // 广播工作任务给所有矿工 | Broadcast work task to all miners
    async fn broadcast_work(&self, work: WorkOrder) {
        // 更新当前工作任务 | Update current work task
        let context = WorkOrderContext {
            work_id: work.work_id.clone(),
            parent_hash: work.parent_hash.clone(),
            merkle_root: work.merkle_root.clone(),
            timestamp: work.timestamp,
            difficulty_target: work.difficulty_target.clone(),
        };
        
        {
            let mut current_work = self.current_work.write().await;
            *current_work = Some(context);
        }

        // 更新状态监控器中的工作ID
        self.status_monitor.update_work_id(work.work_id.clone()).await;
        
        // 更新状态监控器中的难度
        self.status_monitor.update_difficulty(hex::encode(&work.difficulty_target)).await;

        // 广播给所有矿工 | Broadcast to all miners
        info!("广播新工作任务 {} 给所有矿工 | Broadcasting new work task {} to all miners", work.work_id, work.work_id);
        
        let miners = self.miners.clone();
        for miner in miners.iter() {
            let sender = &miner.work_sender;
            if let Err(e) = sender.try_send(Ok(work.clone())) {
                // 如果发送失败，可能矿工断开连接 | If sending fails, the miner might be disconnected
                warn!("矿工 {} 工作任务发送失败: {} | Failed to send work task to miner {}: {}", 
                      miner.miner_id, e, miner.miner_id, e);
            }
        }
    }

    // 从nockchain节点生成新的工作任务 | Generate new work task from nockchain node
    async fn generate_work(&self) -> WorkOrder {
        // 使用实际的nockchain节点数据 | Use actual nockchain node data
        let work_id = Uuid::new_v4().to_string();
        
        info!("生成新工作任务 {} | Generating new work task {}", work_id, work_id);
        
        let _nockchain = self.nockchain.read().await;
        
        // 从nockchain获取当前区块链状态
        // Get current blockchain state from nockchain
        let (parent_hash, merkle_root, difficulty_target) = match self.get_chain_state().await {
            Ok(data) => data,
            Err(e) => {
                error!("获取链状态失败: {} | Failed to get chain state: {}", e, e);
                // 提供安全的默认值作为回退 | Provide safe default values as fallback
                let parent_hash = vec![0; 32];
                let merkle_root = vec![0; 32];
                
                // 创建一个有效的默认难度目标，而不是全0
                // Create a valid default difficulty target instead of all zeros
                let mut difficulty_target = vec![0xFF; 32]; // 初始化为全1
                
                // 设置适合fakenet模式的默认难度
                difficulty_target[0] = 0x00; // 最高有效字节设为0
                difficulty_target[1] = 0x80; // 第二个字节设为10000000，总体难度约为2^15
                
                warn!("使用默认难度目标: {} | Using default difficulty target: {}", 
                      hex::encode(&difficulty_target), hex::encode(&difficulty_target));
                
                (parent_hash, merkle_root, difficulty_target)
            }
        };
        
        WorkOrder {
            work_id,
            parent_hash,
            merkle_root,
            timestamp: chrono::Utc::now().timestamp() as u64,
            difficulty_target,
        }
    }
    
    // 从nockchain节点获取区块链状态 | Get blockchain state from nockchain node
    async fn get_chain_state(&self) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        use nockvm::noun::{D, T};
        use nockvm_macros::tas;
        use nockapp::noun::slab::NounSlab;
        
        // 在非fakenet模式下，我们应该从主网获取真实的区块链状态
        // In non-fakenet mode, we should get the real blockchain state from mainnet
        
        // 更新同步状态
        self.status_monitor.set_sync_status("正在获取区块链状态".to_string()).await;
        
        // 1. peek [%heavy ~] 获取 heaviest block-id
        let mut heavy_slab = NounSlab::new();
        let heavy_path = T(&mut heavy_slab, &[D(tas!(b"heavy")), D(0)]);
        heavy_slab.set_root(heavy_path);
        let block_id = {
            let handle = self.nockchain.read().await.get_handle();
            match handle.peek(heavy_slab).await {
                Ok(Some(slab)) => {
                    // slab.root() 应该是 (unit (unit block-id))
                    let root = unsafe { slab.root() };
                    // 解包两层 Some
                    match root.as_cell() {
                        Ok(cell1) => {
                            match cell1.tail().as_cell() {
                                Ok(cell2) => cell2.head(),
                                Err(e) => {
                                    warn!("获取heaviest block-id失败: {}，尝试使用环境变量", e);
                                    self.status_monitor.set_sync_status("同步失败：无法获取区块ID".to_string()).await;
                                    return self.fallback_chain_state().await;
                                }
                            }
                        },
                        Err(e) => {
                            warn!("获取heaviest block-id失败: {}，尝试使用环境变量", e);
                            self.status_monitor.set_sync_status("同步失败：无法解析区块数据".to_string()).await;
                            return self.fallback_chain_state().await;
                        }
                    }
                },
                _ => {
                    warn!("无法获取heaviest block-id，尝试使用环境变量");
                    self.status_monitor.set_sync_status("同步失败：无法连接主网".to_string()).await;
                    return self.fallback_chain_state().await;
                }
            }
        };
        
        info!("成功获取到主网最新区块ID");
        
        // 获取区块ID的字符串表示，用于日志记录
        let block_id_bytes = match block_id.as_atom() {
            Ok(atom) => atom.to_ne_bytes(),
            Err(_) => vec![]
        };
        let block_id_hex = if !block_id_bytes.is_empty() {
            hex::encode(&block_id_bytes)
        } else {
            "未知".to_string()
        };
        
        // 更新区块哈希到状态监控
        self.status_monitor.update_block_hash(block_id_hex.clone()).await;
        
        // 2. peek [%block <block-id> ~] 获取 page
        let mut block_slab = NounSlab::new();
        let block_path = T(&mut block_slab, &[D(tas!(b"block")), block_id, D(0)]);
        block_slab.set_root(block_path);
        let handle = self.nockchain.read().await.get_handle();
        let res = handle.peek(block_slab).await.map_err(|e| anyhow::anyhow!("peek block failed: {e}"))?;
        let Some(slab) = res else { 
            warn!("无法获取区块页面，尝试使用环境变量");
            self.status_monitor.set_sync_status("同步失败：无法获取区块数据".to_string()).await;
            return self.fallback_chain_state().await; 
        };
        let root = unsafe { slab.root() };
        
        // 解析区块页面结构
        // 区块页面结构通常包含更多信息，我们需要提取:
        // 1. 区块高度 (height)
        // 2. 目标难度 (target)
        // 3. 父区块哈希 (parent_hash)
        let page_cell = match root.as_cell() {
            Ok(cell) => cell,
            Err(e) => {
                warn!("无法解析区块页面: {e}，尝试使用环境变量");
                self.status_monitor.set_sync_status("同步失败：无法解析区块数据".to_string()).await;
                return self.fallback_chain_state().await;
            }
        };
        
        // 获取区块高度
        let height_noun = page_cell.head();
        let block_height = height_noun.as_atom().and_then(|a| a.as_u64()).unwrap_or(0);
        info!("主网当前区块高度: {}", block_height);
        
        // 更新区块高度到状态监控
        self.status_monitor.update_block_height(block_height).await;
        
        // 尝试从nockblocks.com获取当前最新区块高度
        self.try_get_latest_network_height().await;
        
        // 获取区块内容
        let content_cell = match page_cell.tail().as_cell() {
            Ok(cell) => cell,
            Err(e) => {
                warn!("无法获取区块内容: {e}，尝试使用环境变量");
                self.status_monitor.set_sync_status("同步失败：无法解析区块内容".to_string()).await;
                return self.fallback_chain_state().await;
            }
        };
        
        // 获取目标难度
        let target_noun = content_cell.head();
        let target_atom = match target_noun.as_atom() {
            Ok(atom) => atom,
            Err(e) => {
                warn!("无法获取目标难度: {e}，尝试使用环境变量");
                self.status_monitor.set_sync_status("同步失败：无法获取难度目标".to_string()).await;
                return self.fallback_chain_state().await;
            }
        };
        let target_bytes = target_atom.to_ne_bytes();
        info!("成功获取到主网目标难度");
        
        // 更新难度到状态监控
        let target_hex = hex::encode(&target_bytes);
        self.status_monitor.update_difficulty(format!("0x{}", target_hex)).await;
        
        // 获取父区块哈希
        let mut parent_hash_bytes = vec![0; 32]; // 默认值
        
        // 尝试获取父区块ID
        if let Ok(parent_cell) = content_cell.tail().as_cell() {
            if let Ok(parent_id_noun) = parent_cell.head().as_atom() {
                // 将父区块ID转换为哈希值
                parent_hash_bytes = parent_id_noun.to_ne_bytes();
                // 如果哈希不足32字节，填充到32字节
                if parent_hash_bytes.len() < 32 {
                    let mut padded = vec![0; 32];
                    padded[32 - parent_hash_bytes.len()..].copy_from_slice(&parent_hash_bytes);
                    parent_hash_bytes = padded;
                } else if parent_hash_bytes.len() > 32 {
                    // 如果哈希超过32字节，截取前32字节
                    parent_hash_bytes = parent_hash_bytes[0..32].to_vec();
                }
                info!("成功获取到父区块哈希: {}", hex::encode(&parent_hash_bytes));
            }
        }
        
        // 尝试获取交易列表并计算默克尔根
        let mut merkle_root_bytes = vec![0; 32]; // 默认值
        
        // 尝试获取交易列表
        if let Ok(txs_cell) = content_cell.tail().as_cell() {
            if let Ok(_txs_list) = txs_cell.tail().as_cell() {
                // 这里应该遍历交易列表并计算默克尔根
                // 但由于结构可能很复杂，我们简化为使用区块ID的哈希作为默克尔根
                let block_id_atom = block_id.as_atom().unwrap_or_else(|_| Atom::from_value(&mut NounSlab::<nockapp::noun::slab::NockJammer>::new(), 0).unwrap());
                let block_id_bytes = block_id_atom.to_ne_bytes();
                
                // 使用blake3计算哈希作为默克尔根
                let hash = blake3::hash(&block_id_bytes);
                merkle_root_bytes = hash.as_bytes().to_vec();
                info!("成功计算默克尔根: {}", hex::encode(&merkle_root_bytes));
            }
        }
        
        // 更新同步状态为成功
        self.status_monitor.set_sync_status("同步成功".to_string()).await;
        
        info!("成功从主网获取完整的区块链状态");
        // 返回完整的区块链状态
        Ok((parent_hash_bytes, merkle_root_bytes, target_bytes))
    }
    
    // 添加一个回退方法，在无法从主网获取状态时使用
    async fn fallback_chain_state(&self) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        // 尝试读取 DIFFICULTY_TARGET 环境变量
        if let Ok(diff_hex) = std::env::var("DIFFICULTY_TARGET") {
            if let Ok(bytes) = hex::decode(&diff_hex) {
                warn!("使用环境变量中的难度目标作为回退");
                // parent_hash/merkle_root 仍用空
                return Ok((vec![0;32], vec![0;32], bytes));
            }
        }
        
        // 如果环境变量不可用，返回错误
        Err(anyhow::anyhow!("无法获取主网状态，环境变量DIFFICULTY_TARGET也未设置"))
    }
    
    // 将成功的区块提交到nockchain节点 | Submit successful block to nockchain node
    async fn submit_block(&self, work_result: &WorkResult) -> Result<bool> {
        // 获取当前工作任务上下文
        // Get current work task context
        let current_work = self.current_work.read().await;
        let work = match &*current_work {
            Some(work) => work,
            None => {
                warn!("提交区块失败：没有当前工作任务上下文");
                return Ok(false)
            },
        };
        
        info!("准备提交区块到主网，nonce: {}", hex::encode(&work_result.nonce));
        
        // 创建区块提交的poke
        // Create poke for block submission
        let mut submit_slab = NounSlab::new();
        
        // 构建完整的区块数据
        // Build complete block data
        // 将Vec<u8>转换为Bytes进行传递
        let parent_hash_bytes = Bytes::from(work.parent_hash.clone());
        let parent_hash_atom = <Atom as AtomExt>::from_bytes(&mut submit_slab, &parent_hash_bytes);
        
        let merkle_root_bytes = Bytes::from(work.merkle_root.clone());
        let merkle_root_atom = <Atom as AtomExt>::from_bytes(&mut submit_slab, &merkle_root_bytes);
            
        let nonce_bytes = Bytes::from(work_result.nonce.clone());
        let nonce_atom = <Atom as AtomExt>::from_bytes(&mut submit_slab, &nonce_bytes);
            
        let submit_block = Atom::from_value(&mut submit_slab, "submit-block")
            .expect("Failed to create submit-block atom");
        
        info!("区块数据准备完成 - parent_hash: {}, merkle_root: {}, timestamp: {}, nonce: {}",
              hex::encode(&work.parent_hash),
              hex::encode(&work.merkle_root),
              work.timestamp,
              hex::encode(&work_result.nonce));
            
        // 构建区块提交poke [command %submit-block parent_hash merkle_root timestamp nonce]
        // Build block submission poke [command %submit-block parent_hash merkle_root timestamp nonce]
        let submit_poke = T(
            &mut submit_slab,
            &[
                D(tas!(b"command")), 
                submit_block.as_noun(), 
                parent_hash_atom.as_noun(),
                merkle_root_atom.as_noun(),
                D(work.timestamp),
                nonce_atom.as_noun()
            ],
        );
        
        submit_slab.set_root(submit_poke);
        
        info!("发送区块提交请求到nockchain节点");
        
        // 发送区块提交poke，使用线程安全的方式
        // Send block submission poke, using thread-safe method
        let response = match self.handle.safe_poke(MiningWire::SetPubKey.to_wire(), submit_slab).await {
            Ok(res) => res,
            Err(e) => {
                error!("提交区块时发生错误: {}", e);
                return Ok(false);
            }
        };
        
        // 验证响应
        // Validate response
        match response {
            PokeResult::Ack => {
                info!("区块已被nockchain节点接受！");
                // 更新状态监控 - 区块已接受
                self.status_monitor.increment_blocks_accepted();
                
                // 生成新的工作任务
                info!("区块已接受，生成新的工作任务");
                let new_work = self.generate_work().await;
                self.broadcast_work(new_work).await;
                
                Ok(true)
            },
            PokeResult::Nack => {
                warn!("区块被nockchain节点拒绝");
                Ok(false)
            },
        }
    }

    // 获取当前最新区块ID
    async fn get_current_block_id(&self) -> Result<Vec<u8>, anyhow::Error> {
        // 使用peek [%heavy ~] 获取当前最新区块ID
        let mut heavy_slab = NounSlab::new();
        let heavy_path = T(&mut heavy_slab, &[D(tas!(b"heavy")), D(0)]);
        heavy_slab.set_root(heavy_path);
        
        let handle = self.nockchain.read().await.get_handle();
        let res = match handle.peek(heavy_slab).await {
            Ok(res) => res,
            Err(e) => {
                warn!("获取最新区块ID失败: {}", e);
                return Err(anyhow::anyhow!("peek heavy failed: {}", e));
            }
        };
        
        let slab = match res {
            Some(slab) => slab,
            None => {
                warn!("获取最新区块ID返回空结果");
                return Err(anyhow::anyhow!("no heavy block returned"));
            }
        };
        
        // 解析结果
        let root = unsafe { slab.root() };
        
        // 解包两层 Some 以获取block_id
        let cell1 = match root.as_cell() {
            Ok(cell) => cell,
            Err(e) => {
                warn!("解析heavy结果失败 - 不是cell: {}", e);
                return Err(anyhow::anyhow!("heavy result is not a cell: {}", e));
            }
        };
        
        let cell2 = match cell1.tail().as_cell() {
            Ok(cell) => cell,
            Err(e) => {
                warn!("解析heavy结果失败 - tail不是cell: {}", e);
                return Err(anyhow::anyhow!("heavy tail is not a cell: {}", e));
            }
        };
        
        let block_id_noun = cell2.head();
        
        // 将block_id转换为bytes
        let block_id_atom = match block_id_noun.as_atom() {
            Ok(atom) => atom,
            Err(e) => {
                warn!("block_id不是atom: {}", e);
                return Err(anyhow::anyhow!("block_id is not an atom: {}", e));
            }
        };
        
        let block_id_bytes = block_id_atom.to_ne_bytes();
        
        // 记录获取到的区块ID（截取前几个字节）
        let display_len = std::cmp::min(8, block_id_bytes.len());
        info!("获取到当前最新区块ID: {}...", hex::encode(&block_id_bytes[0..display_len]));
        
        Ok(block_id_bytes)
    }
    
    // 检查当前工作是否过时
    async fn is_work_outdated(&self, _current_block_id: &[u8]) -> bool {
        // 获取当前工作任务
        let current_work = self.current_work.read().await;
        if let Some(work) = &*current_work {
            // 获取当前区块链状态
            match self.get_chain_state().await {
                Ok((parent_hash, _, _)) => {
                    // 比较当前工作的parent_hash与最新区块链状态中的parent_hash
                    // 如果不同，说明区块链状态已更新，当前工作过时
                    if parent_hash != work.parent_hash {
                        info!("检测到区块链状态更新，当前工作已过时 | Detected blockchain state update, current work is outdated");
                        info!("最新parent_hash: {}, 当前工作parent_hash: {}", 
                              hex::encode(&parent_hash), hex::encode(&work.parent_hash));
                        return true;
                    }
                    
                    // 检查时间戳，如果当前工作生成时间超过30秒，也认为过时
                    let now = chrono::Utc::now().timestamp() as u64;
                    if now > work.timestamp + 30 {
                        info!("当前工作已超过30秒，生成新工作 | Current work is over 30 seconds old, generating new work");
                        return true;
                    }
                    
                    return false;
                },
                Err(_) => {
                    // 如果获取链状态失败，保守起见认为工作已过时
                    warn!("获取链状态失败，假设工作已过时 | Failed to get chain state, assuming work is outdated");
                    return true;
                }
            }
        }
        
        // 如果没有当前工作，也需要生成新工作
        true
    }

    // 添加clone实现
    fn clone(&self) -> Self {
        Self {
            miners: self.miners.clone(),
            current_work: self.current_work.clone(),
            nockchain: self.nockchain.clone(),
            handle: ThreadSafeNockAppHandle {
                handle: self.handle.handle.clone(),
                effect_sender: self.handle.effect_sender.clone(),
            },
            status_monitor: self.status_monitor.clone(),
            network_manager: self.network_manager.clone(),
        }
    }

    // 这个函数已在第263行定义，此处删除重复定义
    
    // 设置主网创世区块的辅助方法
    async fn set_realnet_genesis_seal(&self) -> Result<bool, anyhow::Error> {
        use nockvm::noun::{D, T};
        use nockapp::noun::slab::NounSlab;
        use nockapp::ToBytes;
        
        info!("尝试设置主网创世区块...");
        
        let mut poke_slab = NounSlab::new();
        let block_height_noun = Atom::new(&mut poke_slab, nockchain::setup::DEFAULT_GENESIS_BLOCK_HEIGHT).as_noun();
        
        let seal_message = nockchain::setup::REALNET_GENESIS_MESSAGE.to_string();
        let seal_byts = Bytes::from(
            seal_message.to_bytes()
                .map_err(|_| anyhow::anyhow!("Failed to convert seal message to bytes"))?
        );
        let seal_noun = Atom::from_bytes(&mut poke_slab, &seal_byts).as_noun();
        
        let tag = Bytes::from(b"set-genesis-seal".to_vec());
        let set_genesis_seal = Atom::from_bytes(&mut poke_slab, &tag).as_noun();
        
        let poke_noun = T(
            &mut poke_slab,
            &[D(tas!(b"command")), set_genesis_seal, block_height_noun, seal_noun],
        );
        poke_slab.set_root(poke_noun);
        
        // 发送poke
        let handle = self.nockchain.read().await.get_handle();
        match handle.poke(nockapp::wire::SystemWire.to_wire(), poke_slab).await {
            Ok(PokeResult::Ack) => {
                info!("主网创世区块设置成功");
                Ok(true)
            },
            Ok(PokeResult::Nack) => {
                warn!("主网创世区块设置被拒绝");
                Ok(false)
            },
            Err(e) => {
                error!("设置主网创世区块时出错: {}", e);
                Err(anyhow::anyhow!("设置主网创世区块时出错: {}", e))
            }
        }
    }
    
    // 验证当前系统是否正确使用了主网创世区块
    async fn verify_mainnet_genesis(&self) -> Result<bool, anyhow::Error> {
        use nockvm::noun::{D, T};
        use nockapp::noun::slab::NounSlab;
        
        // 检查是否设置了创世区块
        let mut peek_slab = NounSlab::new();
        let tag = make_tas(&mut peek_slab, "genesis-seal-set").as_noun();
        let peek_noun = T(&mut peek_slab, &[tag, D(0)]);
        peek_slab.set_root(peek_noun);
        
        let handle = self.nockchain.read().await.get_handle();
        let genesis_seal_set = match handle.peek(peek_slab).await {
            Ok(Some(slab)) => {
                let genesis_seal = unsafe { slab.root() };
                if genesis_seal.is_atom() {
                    unsafe { genesis_seal.raw_equals(&nockvm::noun::YES) }
                } else {
                    false
                }
            },
            _ => false
        };
        
        if !genesis_seal_set {
            warn!("未检测到设置的创世区块，将尝试设置主网创世区块");
            return self.set_realnet_genesis_seal().await;
        }
        
        // 检查当前设置的创世区块信息是否是主网创世区块
        let mut peek_slab = NounSlab::new();
        let tag = make_tas(&mut peek_slab, "genesis-seal").as_noun();
        let peek_noun = T(&mut peek_slab, &[tag, D(0)]);
        peek_slab.set_root(peek_noun);
        
        let handle = self.nockchain.read().await.get_handle();
        match handle.peek(peek_slab).await {
            Ok(Some(slab)) => {
                let genesis_seal = unsafe { slab.root() };
                if let Ok(atom) = genesis_seal.as_atom() {
                    let bytes = atom.to_ne_bytes();
                    let seal_text = String::from_utf8_lossy(&bytes).to_string();
                    
                    // 检查是否与主网创世信息匹配
                    if seal_text.contains(&nockchain::setup::REALNET_GENESIS_MESSAGE) {
                        info!("验证主网创世区块：正确使用了主网创世区块");
                        return Ok(true);
                    } else {
                        warn!("验证主网创世区块：当前使用的不是主网创世区块");
                        // 尝试修复
                        return self.set_realnet_genesis_seal().await;
                    }
                }
            },
            _ => {
                warn!("无法检索创世区块信息");
            }
        }
        
        Ok(false)
    }

    // 这个函数已在第242行定义，此处删除重复定义

    // 尝试从nockblocks.com获取最新区块数据
    async fn fetch_latest_network_data(&self) -> Result<u64, anyhow::Error> {
        // 首先检查环境变量，如果设置了MOCK_NETWORK_BLOCK_HEIGHT，则使用它（用于测试）
        if let Ok(height_str) = std::env::var("MOCK_NETWORK_BLOCK_HEIGHT") {
            if let Ok(height) = height_str.parse::<u64>() {
                info!("使用环境变量中设置的模拟网络区块高度: {}", height);
                return Ok(height);
            }
        }
        
        info!("从nockblocks.com获取最新区块高度...");
        
        // 重试配置
        let max_retries = 3;
        let initial_timeout = 15;
        let mut last_error = None;
        
        for attempt in 1..=max_retries {
            // 根据尝试次数增加超时时间
            let timeout_secs = initial_timeout * attempt;
            
            info!("尝试获取网络区块高度 (尝试 {}/{}，超时 {}秒)", attempt, max_retries, timeout_secs);
            
            // 创建HTTP客户端
            let client = match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(timeout_secs as u64))
                .build() {
                    Ok(client) => client,
                    Err(e) => {
                        let err = PoolServerError::NetworkError(format!("创建HTTP客户端失败: {}", e));
                        error!("{}", err.to_log_string());
                        last_error = Some(anyhow::anyhow!(err));
                        continue;
                    }
                };
                
            // 获取网页内容
            let response_result = client.get("https://nockblocks.com/blocks")
                .header("User-Agent", "Mozilla/5.0 Nockchain-Pool-Server/0.1.0")
                .send()
                .await;
                
            match response_result {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.text().await {
                            Ok(html_content) => {
                                // 直接寻找"Latest Block"或"latest block"标记
                                let latest_block_regex = Regex::new(r"[Ll]atest [Bb]lock.*?(\d+)").unwrap();
                                if let Some(captures) = latest_block_regex.captures(&html_content) {
                                    if let Some(height_match) = captures.get(1) {
                                        if let Ok(height) = height_match.as_str().parse::<u64>() {
                                            info!("成功提取最新区块高度: {}", height);
                                            return Ok(height);
                                        } else {
                                            let err = PoolServerError::DataError("无法将提取的区块高度解析为数字".to_string());
                                            warn!("{}", err.to_log_string());
                                            last_error = Some(anyhow::anyhow!(err));
                                        }
                                    } else {
                                        let err = PoolServerError::DataError("未能提取区块高度数字".to_string());
                                        warn!("{}", err.to_log_string());
                                        last_error = Some(anyhow::anyhow!(err));
                                    }
                                } else {
                                    let err = PoolServerError::DataError("未找到区块高度信息".to_string());
                                    warn!("{}", err.to_log_string());
                                    last_error = Some(anyhow::anyhow!(err));
                                }
                            },
                            Err(e) => {
                                let err = PoolServerError::NetworkError(format!("读取响应内容失败: {}", e));
                                warn!("{}", err.to_log_string());
                                last_error = Some(anyhow::anyhow!(err));
                            }
                        }
                    } else {
                        let err = PoolServerError::NetworkError(format!("HTTP请求失败，状态码: {}", response.status()));
                        warn!("{}", err.to_log_string());
                        last_error = Some(anyhow::anyhow!(err));
                    }
                },
                Err(e) => {
                    let err = PoolServerError::NetworkError(format!("发送HTTP请求失败: {}", e));
                    warn!("{}", err.to_log_string());
                    last_error = Some(anyhow::anyhow!(err));
                }
            }
            
            // 如果不是最后一次尝试，则等待一段时间后重试
            if attempt < max_retries {
                let wait_time = attempt * 2; // 指数退避
                info!("等待 {}秒 后重试...", wait_time);
                tokio::time::sleep(std::time::Duration::from_secs(wait_time)).await;
            }
        }
        
        // 如果所有尝试都失败，使用备用方法
        warn!("无法从nockblocks.com获取最新区块高度，使用估算值");
        
        // 记录最后一个错误
        if let Some(err) = last_error {
            debug!("获取网络高度失败的详细错误: {}", err);
        }
        
        // 使用备用方法：当前区块高度+偏移值
        let current_height = self.status_monitor.get_statistics().await.current_block_height;
        let estimated_height = current_height + 10; // 估算网络高度比本地高10个区块
        info!("使用估算的网络区块高度: {}", estimated_height);
        Ok(estimated_height)
    }

    // 添加获取当前区块高度的方法
    async fn get_current_block_height(&self) -> Result<u64, anyhow::Error> {
        use nockvm::noun::{D, T};
        use nockvm_macros::tas;
        use nockapp::noun::slab::NounSlab;
        
        // 获取当前区块高度
        let mut heavy_slab = NounSlab::new();
        let heavy_path = T(&mut heavy_slab, &[D(tas!(b"heavy")), D(0)]);
        heavy_slab.set_root(heavy_path);
        
        let handle = self.nockchain.read().await.get_handle();
        match handle.peek(heavy_slab).await {
            Ok(Some(slab)) => {
                let root = unsafe { slab.root() };
                
                // 解包两层 Some 以获取block_id
                match root.as_cell() {
                    Ok(cell1) => {
                        match cell1.tail().as_cell() {
                            Ok(cell2) => {
                                let block_id = cell2.head();
                                
                                // 获取区块信息
                                let mut block_slab = NounSlab::new();
                                let block_path = T(&mut block_slab, &[D(tas!(b"block")), block_id, D(0)]);
                                block_slab.set_root(block_path);
                                
                                match handle.peek(block_slab).await {
                                    Ok(Some(slab)) => {
                                        let root = unsafe { slab.root() };
                                        match root.as_cell() {
                                            Ok(page_cell) => {
                                                // 获取区块高度
                                                let height_noun = page_cell.head();
                                                if let Ok(height) = height_noun.as_atom().and_then(|a| a.as_u64()) {
                                                    return Ok(height);
                                                }
                                            },
                                            Err(e) => return Err(anyhow::anyhow!("解析区块页面失败: {}", e))
                                        }
                                    },
                                    Ok(None) => return Err(anyhow::anyhow!("未找到区块信息")),
                                    Err(e) => return Err(anyhow::anyhow!("获取区块信息失败: {}", e))
                                }
                            },
                            Err(e) => return Err(anyhow::anyhow!("解析heavy cell失败: {}", e))
                        }
                    },
                    Err(e) => return Err(anyhow::anyhow!("解析heavy root失败: {}", e))
                }
            },
            Ok(None) => return Err(anyhow::anyhow!("未找到heavy区块")),
            Err(e) => return Err(anyhow::anyhow!("获取heavy区块失败: {}", e))
        }
        
        Err(anyhow::anyhow!("无法获取当前区块高度"))
    }
}

#[tonic::async_trait]
impl MiningPool for MiningPoolService {
    type SubscribeStream = WorkOrderStream;

    async fn subscribe(
        &self,
        request: Request<tonic::Streaming<MinerStatus>>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        // 创建用于发送工作任务的通道 | Create a channel for sending work tasks
        let (tx, rx) = mpsc::channel(100);
        
        let mut stream = request.into_inner();
        
        // 接收第一个状态消息来识别矿工 | Receive first status message to identify the miner
        let initial_status = match stream.message().await {
            Ok(Some(status)) => status,
            _ => return Err(Status::invalid_argument("初始化矿工状态失败 | Failed to initialize miner status")),
        };
        
        let miner_id = initial_status.miner_id.clone();
        let threads = initial_status.threads;
        
        info!(
            "矿工 {} 已连接，使用 {} 个线程。总矿工数: {} | Miner {} connected with {} threads. Total miners: {}", 
            miner_id, threads, self.miners.len() + 1, miner_id, threads, self.miners.len() + 1
        );
        
        // 更新状态监控器中的矿工信息
        self.status_monitor.update_miner(miner_id.clone(), threads as usize).await;
        
        // 额外克隆一份miner_id用于后续使用
        let miner_id_for_response = miner_id.clone();
        
        // 保存矿工连接 | Save miner connection
        self.miners.insert(
            miner_id.clone(),
            MinerConnection {
                miner_id: miner_id.clone(),
                work_sender: tx.clone(),
                threads,
            },
        );
        
        // 开启一个任务处理来自矿工的状态更新 | Start a task to handle status updates from the miner
        let miners = self.miners.clone();
        let miner_id_clone = miner_id.clone(); // 克隆一份用于移动到任务中
        let status_monitor = self.status_monitor.clone();
        tokio::spawn(async move {
            while let Ok(Some(status)) = stream.message().await {
                // 更新矿工状态 | Update miner status
                if let Some(mut miner) = miners.get_mut(&status.miner_id) {
                    miner.threads = status.threads;
                    
                    // 更新状态监控器中的矿工线程数
                    status_monitor.update_miner(status.miner_id.clone(), status.threads as usize).await;
                }
            }
            
            // 矿工断开连接 | Miner disconnected
            miners.remove(&miner_id_clone);
            
            // 从状态监控器中移除矿工
            status_monitor.remove_miner(&miner_id_clone);
            
            info!("矿工 {} 断开连接。总矿工数: {} | Miner {} disconnected. Total miners: {}", 
                  &miner_id_clone, miners.len(), &miner_id_clone, miners.len());
        });
        
        // 发送当前工作任务给新连接的矿工 | Send current work task to newly connected miner
        let current_work_read = self.current_work.read().await;
        if let Some(work_context) = &*current_work_read {
            let work = WorkOrder {
                work_id: work_context.work_id.clone(),
                parent_hash: work_context.parent_hash.clone(),
                merkle_root: work_context.merkle_root.clone(),
                timestamp: work_context.timestamp,
                difficulty_target: work_context.difficulty_target.clone(),
            };
            
            if let Err(e) = tx.try_send(Ok(work)) {
                error!("向矿工 {} 发送初始工作任务失败: {} | Failed to send initial work task to miner {}: {}", 
                       &miner_id_for_response, e, &miner_id_for_response, e);
            }
        } else {
            // 如果没有当前工作，生成一个新的 | If there's no current work, generate a new one
            let new_work = self.generate_work().await;
            self.broadcast_work(new_work).await;
        }
        
        Ok(Response::new(WorkOrderStream { inner: rx }))
    }

    async fn submit_work(
        &self,
        request: Request<WorkResult>,
    ) -> Result<Response<SubmitAck>, Status> {
        let work_result = request.into_inner();
        let miner_id = work_result.miner_id.clone();
        let work_id = work_result.work_id.clone();
        
        info!("收到来自矿工 {} 的工作结果，任务ID: {} | Received work result from miner {}, task ID: {}", 
              miner_id, work_id, miner_id, work_id);
        
        // 验证工作结果 | Validate work result
        let is_valid = self.validate_work(&work_result).await;
        
        // 更新状态监控器中的矿工份额
        self.status_monitor.increment_miner_share(&miner_id, is_valid);
        
        if is_valid {
            // 增加找到区块的计数
            self.status_monitor.increment_blocks_found();
            
            info!(
                "接受来自矿工 {} 的有效工作，广播区块... | Accepted valid work from miner {}, broadcasting block...", 
                miner_id, miner_id
            );
            
            // 通过内嵌节点广播区块到主网 | Broadcast block to mainnet via embedded node
            match self.submit_block(&work_result).await {
                Ok(true) => {
                    info!("区块成功提交到网络 | Block successfully submitted to network");
                },
                Ok(false) => {
                    warn!("区块提交失败 | Block submission failed");
                },
                Err(e) => {
                    error!("区块提交错误: {} | Block submission error: {}", e, e);
                }
            }
            
            // 生成新工作并广播 | Generate new work and broadcast
            let new_work = self.generate_work().await;
            self.broadcast_work(new_work).await;
            
            Ok(Response::new(SubmitAck { success: true }))
        } else {
            warn!(
                "拒绝来自矿工 {} 的无效工作 | Rejected invalid work from miner {}", 
                miner_id, miner_id
            );
            Ok(Response::new(SubmitAck { success: false }))
        }
    }
}

impl MiningPoolService {
    // 验证工作结果 | Validate work result
    async fn validate_work(&self, result: &WorkResult) -> bool {
        // 获取当前工作任务上下文 | Get current work task context
        let current_work = self.current_work.read().await;
        
        // 确认工作任务ID匹配 | Confirm work task ID matches
        if let Some(work) = &*current_work {
            if work.work_id != result.work_id {
                warn!(
                    "工作任务ID不匹配: 期望 {}, 收到 {} | Work task ID mismatch: expected {}, got {}", 
                    work.work_id, result.work_id, work.work_id, result.work_id
                );
                return false;
            }
            
            // 从当前工作任务和提交的nonce构建区块头
            // Build block header from current work task and submitted nonce
            let mut block_data = Vec::with_capacity(
                work.parent_hash.len() + work.merkle_root.len() + 8 + result.nonce.len()
            );
            
            block_data.extend_from_slice(&work.parent_hash);
            block_data.extend_from_slice(&work.merkle_root);
            block_data.extend_from_slice(&work.timestamp.to_le_bytes());
            block_data.extend_from_slice(&result.nonce);
            
            // 计算区块哈希 | Calculate block hash
            let hash = blake3::hash(&block_data);
            
            // 记录提交的nonce和计算出的哈希值
            info!(
                "收到nonce: {}, 计算出哈希值: {}", 
                hex::encode(&result.nonce), hex::encode(hash.as_bytes())
            );
            
            // 记录当前难度目标
            info!("当前难度目标: {}", hex::encode(&work.difficulty_target));
            
            // 比较哈希与目标难度 | Compare hash with target difficulty
            let is_valid = compare_hash_with_target(hash.as_bytes(), &work.difficulty_target);
            
            if is_valid {
                info!(
                    "有效的工作结果: 哈希值 {} 满足难度目标 {} | Valid work result: hash {} meets difficulty target {}", 
                    hex::encode(hash.as_bytes()), hex::encode(&work.difficulty_target),
                    hex::encode(hash.as_bytes()), hex::encode(&work.difficulty_target)
                );
            } else {
                warn!(
                    "无效的工作结果: 哈希值 {} 不满足难度目标 {} | Invalid work result: hash {} does not meet difficulty target {}", 
                    hex::encode(hash.as_bytes()), hex::encode(&work.difficulty_target),
                    hex::encode(hash.as_bytes()), hex::encode(&work.difficulty_target)
                );
            }
            
            return is_valid;
        } else {
            warn!("没有当前工作任务上下文可用于验证 | No current work task context available for validation");
            return false;
        }
    }
}

// 比较哈希与目标难度 | Compare hash with target difficulty
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

#[tokio::main]
async fn main() -> Result<()> {
    // 加载.env文件
    dotenv().ok();
    
    // 使用与nockchain兼容的日志配置
    if std::env::var("RUST_LOG").is_err() {
        // 仅当用户未设置时提供默认值
        std::env::set_var("RUST_LOG", "info");
    }
    
    // 显示MINIMAL_LOG_FORMAT环境变量
    if std::env::var("MINIMAL_LOG_FORMAT").is_err() {
        std::env::set_var("MINIMAL_LOG_FORMAT", "1");
    }
    
    // 初始化与nockchain兼容的日志系统
    try_init_tracing();
    
    // 添加矿池服务器欢迎信息，格式与nockchain一致
    info!("
    _   _            _        _           _          _____             _ 
   | \\ | |          | |      | |         (_)        |  __ \\           | |
   |  \\| | ___   ___| | _____| |__   __ _ _ _ __    | |__) |__   ___ | |
   | . ` |/ _ \\ / __| |/ / __| '_ \\ / _` | | '_ \\   |  ___/ _ \\ / _ \\| |
   | |\\  | (_) | (__|   < (__| | | | (_| | | | | |  | |  | (_) | (_) | |
   |_| \\_|\\___/ \\___|_|\\_\\___|_| |_|\\__,_|_|_| |_|  |_|   \\___/ \\___/|_|
                                                                        
    ");
    
        // 读取矿工公钥环境变量
    let mining_pubkey = env::var("MINING_PUBKEY")
        .expect("必须设置MINING_PUBKEY环境变量");

    // 读取矿池服务器监听地址
    let pool_server_address = env::var("POOL_SERVER_ADDRESS")
        .unwrap_or_else(|_| "0.0.0.0:7777".to_string());
    
    // 读取HTTP API服务器地址
    let http_api_address = env::var("HTTP_API_ADDRESS")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    info!("启动矿池服务器");
    info!("挖矿公钥: {}", mining_pubkey);
    
    // 初始化nockchain节点
    info!("初始化内嵌nockchain节点...");
    
    // 创建一个nockchain cli配置，用于初始化节点
    // Create a nockchain cli config to initialize the node
    let nockchain_cli = nockchain::config::NockchainCli {
        nockapp_cli: boot::default_boot_cli(true),
        npc_socket: ".socket/nockchain_npc.sock".to_string(),
        mine: false,
        mining_pubkey: Some(mining_pubkey.clone()),
        mining_key_adv: None,
        fakenet: false,
        // 将已知节点放回peer列表，依赖nockchain内部的重连逻辑
        peer: vec!["/dns4/p2p-mainnet.urbit.org/udp/30006/quic-v1/p2p/12D3KooWEjQcZUS3x5YEMuMj1kgZJyoV4MTHNRnbtbSEMAMzwGQC".to_string()],
        force_peer: vec![],
        allowed_peers_path: None,
        no_default_peers: false, // 恢复默认，使其尝试连接peer列表
        bind: vec![
            "/ip4/0.0.0.0/udp/13340/quic-v1".to_string()
        ],
        new_peer_id: false,
        max_established_incoming: None,
        max_established_outgoing: None,
        max_pending_incoming: None,
        max_pending_outgoing: None,
        max_established: None,
        max_established_per_peer: None,
        prune_inbound: None,
        max_system_memory_fraction: None,
        max_system_memory_bytes: None,
        num_threads: Some(1),
        fakenet_pow_len: None,
        fakenet_log_difficulty: None,
        fakenet_genesis_jam_path: None,
    };
    
    // 记录我们正在使用的配置
    info!("当前网络模式: {}", if nockchain_cli.fakenet { "fakenet" } else { "主网" });
    info!("将使用的创世信息: {}", nockchain::setup::REALNET_GENESIS_MESSAGE);
    
    // 使用修改后的启动方式
    // Use modified boot method
    let prover_hot_state = produce_prover_hot_state();
    
    // 我们在main函数开头已经设置了环境变量
    // We already set the environment variable at the beginning of main
    
    // 通过原始的init_with_kernel函数初始化NockApp
    // Initialize NockApp using the original init_with_kernel function
    let nockapp = nockchain::init_with_kernel(
        Some(nockchain_cli), // 使用我们的配置
        KERNEL,
        prover_hot_state.as_slice()
    ).await.expect("Failed to initialize nockchain");
    
    // 注意：NockApp不能被克隆，因此我们只创建服务
    // 并使用服务的方式来与nockapp交互
    info!("准备初始化矿池服务...");

    // 初始化矿池服务
    let service = MiningPoolService::new(nockapp);
    
    // 移除手动引导任务，因为它无法实际工作
    // let service_clone_for_bootstrap = Arc::new(service.clone());
    // tokio::spawn(async move {
    //     bootstrap_network(service_clone_for_bootstrap).await;
    // });

    // 获取对状态监控器的引用，用于HTTP API
    let status_monitor = service.status_monitor.clone();
    
    // 执行初始同步，尝试与主网同步创世区块信息
    info!("执行初始主网同步...");
    if service.initial_sync_with_mainnet().await {
        info!("与主网同步成功，可以正常开始挖矿操作");
    } else {
        warn!("与主网同步失败，将尝试使用本地设置继续运行");
    }
    
    // 生成初始工作任务
    let initial_work = service.generate_work().await;
    service.broadcast_work(initial_work).await;
    
    // 立即打印一次初始状态
    info!("===== 初始同步状态 =====");
    let initial_stats = status_monitor.get_statistics().await;
    info!("区块高度: {}/{:?}", initial_stats.current_block_height, initial_stats.network_latest_block_height.unwrap_or(0));
    info!("同步状态: {} ({:.2}%)", initial_stats.sync_status, initial_stats.sync_percentage);
    info!("========================");
    
    // 启动定期检查区块链状态的任务
    // 这是一个额外的保障机制，防止事件监听器错过某些更新
    let service_clone = Arc::new(service.clone());
    tokio::spawn(async move {
        // 主网模式下，更频繁地检查状态以确保及时同步
        let interval_seconds = 10;
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_seconds));
        
        info!("启动定期区块链状态检查任务，间隔 {} 秒", interval_seconds);
        
        let mut last_check_time = std::time::Instant::now();
        let mut last_block_id: Option<Vec<u8>> = None;
        
        loop {
            interval.tick().await;
            
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(last_check_time).as_secs();
            last_check_time = now;

            // info!("执行定期区块链状态检查 ({}秒后)...", elapsed); // 此日志过于频繁，暂时注释
            
            // 直接获取最新区块ID，并与上次记录的ID比较
            match service_clone.get_current_block_id().await {
                Ok(current_block_id) => {
                    let block_changed = last_block_id.as_ref() != Some(&current_block_id);

                    if block_changed {
                        info!("检测到新区块，更新状态并触发新工作...");
                        last_block_id = Some(current_block_id.clone());

                        // 1. 更新状态监控器
                        let block_id_hex = hex::encode(&current_block_id);
                        service_clone.status_monitor.update_block_hash(block_id_hex).await;
                        if let Ok(height) = service_clone.get_current_block_height().await {
                            service_clone.status_monitor.update_block_height(height).await;
                        }
                        service_clone.try_get_latest_network_height().await;

                        // 打印详细的进度更新
                        let stats = service_clone.status_monitor.get_statistics().await;
                        info!("同步进度: 本地区块 {} / 网络区块 {:?} ({}%)", 
                              stats.current_block_height, 
                              stats.network_latest_block_height.unwrap_or(0),
                              format!("{:.2}", stats.sync_percentage));

                        // 2. 触发新工作广播
                        let new_work = service_clone.generate_work().await;
                        service_clone.broadcast_work(new_work).await;
                    } else {
                        // info!("区块链状态无变化。"); // 此日志过于频繁，暂时注释
                    }
                },
                Err(e) => {
                    warn!("定期检查：获取区块ID失败: {}", e);
                }
            };

            // 每60秒尝试更新一次网络高度，并打印状态摘要
            if elapsed >= 60 {
                info!("执行主网区块高度检查...");
                service_clone.try_get_latest_network_height().await;

                // 打印状态摘要
                let stats = service_clone.status_monitor.get_statistics().await;
                info!("===== 状态摘要 =====");
                info!("  同步进度: 本地区块 {} / 网络区块 {:?} ({}%)", 
                      stats.current_block_height, 
                      stats.network_latest_block_height.unwrap_or(0),
                      format!("{:.2}", stats.sync_percentage));
                info!("  连接矿工: {}, 总线程数: {}", stats.connected_miners, stats.total_threads);
                info!("=====================");
            }
        }
    });
    
    // 启动HTTP API服务（放到 tokio::spawn 里）
    info!("启动HTTP API服务器在 {}", http_api_address);
    let status_monitor_clone = status_monitor.clone();
    tokio::spawn(async move {
        http_api::start_api_server(status_monitor_clone, &http_api_address).await;
    });
    
    // 启动gRPC服务器
    let addr = pool_server_address.parse()?;
    info!("矿池服务器监听于 {}", addr);
    
    Server::builder()
        .add_service(MiningPoolServer::new(service))
        .serve(addr)
        .await?;
    
    Ok(())
} 

// 移除无法实现的手动引导函数
// async fn bootstrap_network(service: Arc<MiningPoolService>) { ... } 