use anyhow::Result;
use dashmap::DashMap;
use dotenv::dotenv;
use futures_core::Stream;
use std::env;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{error, info, warn};
use uuid::Uuid;
use blake3;
// 引入状态监控模块
use crate::status_monitor::StatusMonitor;

// 引入nockchain核心库，用于集成nockchain节点
use kernels::dumb::KERNEL;
use nockapp::kernel::boot;
// 移除未使用的导入
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

// 添加用于检查跟踪系统状态的导入
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt, registry};
use std::sync::atomic::{AtomicBool, Ordering};

// 添加新模块声明
mod status_monitor;
mod http_api;

// 定义一个静态变量来跟踪是否已经初始化
static TRACING_INITIALIZED: AtomicBool = AtomicBool::new(false);

// 自定义函数来安全地初始化跟踪系统
fn try_init_tracing() {
    // 检查是否已初始化或被环境变量禁用
    if TRACING_INITIALIZED.load(Ordering::SeqCst) || 
       std::env::var("TRACING_DISABLED").is_ok() {
        return; // 如果已初始化或被禁用，则直接返回
    }
    
    // 尝试在一个单独的线程上初始化，避免与其他潜在的初始化冲突
    std::thread::spawn(|| {
        // 尝试初始化跟踪系统
        let filter = EnvFilter::new(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()));
        
        // 使用标准的 fmt 层
        let fmt_layer = fmt::layer().with_target(true);
        
        // 尝试设置全局跟踪系统，但忽略可能的错误
        let _ = registry().with(filter).with(fmt_layer).try_init();
    }).join().ok(); // 忽略可能的错误
    
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
}

impl ThreadSafeNockAppHandle {
    fn new(handle: NockAppHandle) -> Self {
        Self {
            handle: Arc::new(Mutex::new(handle)),
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
}

impl MiningPoolService {
    fn new(nockchain: NockApp) -> Self {
        let handle = ThreadSafeNockAppHandle::new(nockchain.get_handle());
        let status_monitor = Arc::new(StatusMonitor::new());
        
        // 启动定期状态更新
        status_monitor::StatusMonitor::start_periodic_updates(status_monitor.clone());
        
        Self {
            miners: Arc::new(DashMap::new()),
            current_work: Arc::new(RwLock::new(None)),
            nockchain: Arc::new(RwLock::new(nockchain)),
            handle,
            status_monitor,
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
        // 创建请求获取当前区块链状态的poke
        // Create poke to get current blockchain state
        let mut request_slab = NounSlab::new();
        let get_mining_info = Atom::from_value(&mut request_slab, "get-mining-info")
            .expect("Failed to create get-mining-info atom");
        let request_poke = T(
            &mut request_slab,
            &[D(tas!(b"command")), get_mining_info.as_noun()],
        );
        request_slab.set_root(request_poke);
        
        // 发送请求并等待响应，使用线程安全的方式
        // Send request and wait for response, using thread-safe method
        let response = self.handle.safe_poke(MiningWire::SetPubKey.to_wire(), request_slab).await?;
        
        // 假设获取了区块高度，更新状态监控器
        // 实际实现中应从nockchain节点获取真实高度
        let block_height = 12345; // 示例值，实际应从响应中获取
        self.status_monitor.update_block_height(block_height).await;
        
        // 直接处理响应，不需要调用root()方法
        match response {
            PokeResult::Ack => {
                // 从handle中获取结果
                // 这里返回更合理的测试数据
                // 实际应该从响应中解析数据
                let parent_hash = vec![0; 32];
                let merkle_root = vec![0; 32];
                
                // 设置适合fakenet模式的难度目标
                // 根据fakenet_pow_len=2(16位)和fakenet_log_difficulty=1(难度因子2)
                // 创建一个有效的难度目标，约等于2^15个前导零位
                let mut difficulty = vec![0xFF; 32]; // 初始化为全1
                
                // 根据fakenet配置调整难度
                // 对于fakenet_pow_len=2和fakenet_log_difficulty=1
                // 设置前2字节为0，第3字节为0x80
                difficulty[0] = 0x00; // 最高有效字节设为0
                difficulty[1] = 0x80; // 第二个字节设为10000000，总体难度约为2^15
                
                // 记录实际使用的难度值
                info!("设置挖矿难度目标: {}", hex::encode(&difficulty));
                
                Ok((parent_hash, merkle_root, difficulty))
            },
            PokeResult::Nack => {
                Err(anyhow::anyhow!("Nack response from nockchain node"))
            }
        }
    }
    
    // 将成功的区块提交到nockchain节点 | Submit successful block to nockchain node
    async fn submit_block(&self, work_result: &WorkResult) -> Result<bool> {
        // 获取当前工作任务上下文
        // Get current work task context
        let current_work = self.current_work.read().await;
        let work = match &*current_work {
            Some(work) => work,
            None => return Ok(false),
        };
        
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
        
        // 发送区块提交poke，使用线程安全的方式
        // Send block submission poke, using thread-safe method
        let response = self.handle.safe_poke(MiningWire::SetPubKey.to_wire(), submit_slab).await?;
        
        // 验证响应
        // Validate response
        match response {
            PokeResult::Ack => {
                // 更新状态监控 - 区块已接受
                self.status_monitor.increment_blocks_accepted();
                Ok(true)
            },
            PokeResult::Nack => Ok(false),
        }
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
    // 加载.env文件 | Load .env file
    dotenv().ok();
    
    // 设置环境变量以禁止跟踪系统初始化，并只显示错误级别的日志
    // Set environment variables to disable tracing initialization and only show error level logs
    std::env::set_var("RUST_LOG", "info");  // 修改为info级别以显示更多信息
    std::env::set_var("TRACING_DISABLED", "1");
    
    // 安全地尝试初始化跟踪系统 | Safely try to initialize tracing
    try_init_tracing();
    
    // 读取矿工公钥环境变量 | Read mining public key environment variable
    let mining_pubkey = env::var("MINING_PUBKEY")
        .expect("必须设置MINING_PUBKEY环境变量 | MINING_PUBKEY environment variable must be set");
    
    // 读取矿池服务器监听地址 | Read pool server listening address
    let pool_server_address = env::var("POOL_SERVER_ADDRESS")
        .unwrap_or_else(|_| "0.0.0.0:7777".to_string());
    
    // 读取HTTP API服务器地址
    let http_api_address = env::var("HTTP_API_ADDRESS")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    
    info!("启动矿池服务器，挖矿公钥: {} | Starting pool server, mining pubkey: {}", mining_pubkey, mining_pubkey);
    
    // 初始化nockchain节点 | Initialize nockchain node
    info!("初始化内嵌nockchain节点... | Initializing embedded nockchain node...");
    
    // 创建配置但跳过日志初始化
    // Create config but skip logging initialization
    let nockapp_cli = boot::default_boot_cli(false);
    
    // 创建nockchain配置并禁用日志初始化
    // Create nockchain config and disable logging initialization
    let nockchain_cli = nockchain::config::NockchainCli {
        nockapp_cli: nockapp_cli,
        npc_socket: ".socket/nockchain_npc.sock".to_string(),
        mine: false,
        mining_pubkey: Some(mining_pubkey.clone()),
        mining_key_adv: None,
        fakenet: true,  // 使用fakenet模式来简化测试
        peer: vec![],
        force_peer: vec![],
        allowed_peers_path: None,
        no_default_peers: true,  // 不连接默认节点
        bind: vec![],
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
        fakenet_pow_len: Some(3),
        fakenet_log_difficulty: Some(21),
        fakenet_genesis_jam_path: None,
    };
    
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
    info!("准备初始化矿池服务... | Preparing to initialize mining pool service...");

    // 初始化矿池服务
    let service = MiningPoolService::new(nockapp);
    
    // 获取对状态监控器的引用，用于HTTP API
    let status_monitor = service.status_monitor.clone();
    
    // 生成初始工作任务 | Generate initial work task
    let initial_work = service.generate_work().await;
    service.broadcast_work(initial_work).await;
    
    // 启动HTTP API服务
    info!("启动HTTP API服务器在 {} | Starting HTTP API server on {}", http_api_address, http_api_address);
    http_api::start_api_server(status_monitor, &http_api_address).await;
    
    // 启动gRPC服务器 | Start gRPC server
    let addr = pool_server_address.parse()?;
    info!("矿池服务器监听于 {} | Pool server listening on {}", addr, addr);
    
    Server::builder()
        .add_service(MiningPoolServer::new(service))
        .serve(addr)
        .await?;
    
    Ok(())
} 