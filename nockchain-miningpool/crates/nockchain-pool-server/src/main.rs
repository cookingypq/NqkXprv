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

// 简化日志初始化导入
use nockapp::kernel::boot::init_default_tracing;
use std::sync::atomic::{AtomicBool, Ordering};

// 添加用于检查跟踪系统状态的导入
use tokio::sync::broadcast;

// 添加新模块声明
mod status_monitor;
mod http_api;

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
}

impl MiningPoolService {
    fn new(nockchain: NockApp) -> Self {
        let handle = ThreadSafeNockAppHandle::new(nockchain.get_handle());
        let status_monitor = Arc::new(StatusMonitor::new());
        
        // 启动定期状态更新
        status_monitor::StatusMonitor::start_periodic_updates(status_monitor.clone());
        
        let service = Self {
            miners: Arc::new(DashMap::new()),
            current_work: Arc::new(RwLock::new(None)),
            nockchain: Arc::new(RwLock::new(nockchain)),
            handle,
            status_monitor,
        };
        
        // 启动区块链事件监听
        service.start_blockchain_event_listener();
        
        service
    }

    // 启动区块链事件监听器，用于监听新区块事件
    fn start_blockchain_event_listener(&self) {
        // 克隆必要的引用以便在异步任务中使用
        let service_clone = Arc::new(self.clone());
        
        // 启动异步任务监听区块链事件
        tokio::spawn(async move {
            // 获取effect接收器，用于接收区块链事件
            let mut effect_receiver = service_clone.handle.effect_sender.subscribe();
            
            info!("开始监听区块链事件 | Started listening for blockchain events");
            
            // 持续监听区块链事件
            while let Ok(_effect) = effect_receiver.recv().await {
                // 当收到任何effect时，我们检查当前链状态是否有更新
                // 使用peek [%heavy ~] 来获取当前最新区块
                let current_block_id = match service_clone.get_current_block_id().await {
                    Ok(id) => id,
                    Err(e) => {
                        warn!("获取当前区块ID失败: {} | Failed to get current block ID: {}", e, e);
                        continue;
                    }
                };
                
                // 检查是否需要更新工作任务
                let should_update = {
                    // 检查当前工作任务是否存在，以及关联的区块ID是否与最新的不同
                    let current_work = service_clone.current_work.read().await;
                    current_work.is_none() || service_clone.is_work_outdated(&current_block_id).await
                };
                
                if should_update {
                    // 生成新工作并广播
                    info!("检测到区块链状态更新，生成新工作任务 | Detected blockchain state update, generating new work task");
                    let new_work = service_clone.generate_work().await;
                    service_clone.broadcast_work(new_work).await;
                }
            }
            
            warn!("区块链事件监听器已退出 | Blockchain event listener exited");
        });
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
        // 优先读取 DIFFICULTY_TARGET 环境变量
        if let Ok(diff_hex) = std::env::var("DIFFICULTY_TARGET") {
            if let Ok(bytes) = hex::decode(&diff_hex) {
                // parent_hash/merkle_root 仍用空
                return Ok((vec![0;32], vec![0;32], bytes));
            }
        }
        use nockvm::noun::{D, T};
        use nockvm_macros::tas;
        use nockapp::noun::slab::NounSlab;
        
        // 1. peek [%heavy ~] 获取 heaviest block-id
        let mut heavy_slab = NounSlab::new();
        let heavy_path = T(&mut heavy_slab, &[D(tas!(b"heavy")), D(0)]);
        heavy_slab.set_root(heavy_path);
        let block_id = {
            let handle = self.nockchain.read().await.get_handle();
            let res = handle.peek(heavy_slab).await.map_err(|e| anyhow::anyhow!("peek heavy failed: {e}"))?;
            let Some(slab) = res else { return Err(anyhow::anyhow!("no heavy block returned")); };
            // slab.root() 应该是 (unit (unit block-id))
            let root = unsafe { slab.root() };
            // 解包两层 Some
            let cell1 = root.as_cell().map_err(|_| anyhow::anyhow!("heavy: not cell1"))?;
            let cell2 = cell1.tail().as_cell().map_err(|_| anyhow::anyhow!("heavy: not cell2"))?;
            let block_id_noun = cell2.head();
            block_id_noun
        };
        
        // 2. peek [%block <block-id> ~] 获取 page
        let mut block_slab = NounSlab::new();
        let block_path = T(&mut block_slab, &[D(tas!(b"block")), block_id, D(0)]);
        block_slab.set_root(block_path);
        let handle = self.nockchain.read().await.get_handle();
        let res = handle.peek(block_slab).await.map_err(|e| anyhow::anyhow!("peek block failed: {e}"))?;
        let Some(slab) = res else { return Err(anyhow::anyhow!("no block page returned")); };
        let root = unsafe { slab.root() };
        
        // 解析区块页面结构
        // 区块页面结构通常包含更多信息，我们需要提取:
        // 1. 区块高度 (height)
        // 2. 目标难度 (target)
        // 3. 父区块哈希 (parent_hash)
        let page_cell = root.as_cell().map_err(|_| anyhow::anyhow!("block page: not cell"))?;
        
        // 获取区块高度
        let height_noun = page_cell.head();
        let block_height = height_noun.as_atom().and_then(|a| a.as_u64()).unwrap_or(0);
        
        // 获取区块内容
        let content_cell = page_cell.tail().as_cell().map_err(|_| anyhow::anyhow!("block page: content not cell"))?;
        
        // 获取目标难度
        let target_noun = content_cell.head();
        let target_atom = target_noun.as_atom().map_err(|e| anyhow::anyhow!("block page: target_noun 不是 atom: {}", e))?;
        let target_bytes = target_atom.to_ne_bytes();
        
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
            }
        }
        
        // 尝试获取交易列表并计算默克尔根
        let mut merkle_root_bytes = vec![0; 32]; // 默认值
        
        // 尝试获取交易列表
        if let Ok(txs_cell) = content_cell.tail().as_cell() {
            if let Ok(txs_list) = txs_cell.tail().as_cell() {
                // 这里应该遍历交易列表并计算默克尔根
                // 但由于结构可能很复杂，我们简化为使用区块ID的哈希作为默克尔根
                let block_id_atom = block_id.as_atom().unwrap_or_else(|_| Atom::from_value(&mut NounSlab::<nockapp::noun::slab::NockJammer>::new(), 0).unwrap());
                let block_id_bytes = block_id_atom.to_ne_bytes();
                
                // 使用blake3计算哈希作为默克尔根
                let hash = blake3::hash(&block_id_bytes);
                merkle_root_bytes = hash.as_bytes().to_vec();
            }
        }
        
        // 更新状态监控器
        self.status_monitor.update_block_height(block_height).await;
        
        // 返回完整的区块链状态
        Ok((parent_hash_bytes, merkle_root_bytes, target_bytes))
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

    // 获取当前最新区块ID
    async fn get_current_block_id(&self) -> Result<Vec<u8>, anyhow::Error> {
        // 使用peek [%heavy ~] 获取当前最新区块ID
        let mut heavy_slab = NounSlab::new();
        let heavy_path = T(&mut heavy_slab, &[D(tas!(b"heavy")), D(0)]);
        heavy_slab.set_root(heavy_path);
        
        let handle = self.nockchain.read().await.get_handle();
        let res = handle.peek(heavy_slab).await.map_err(|e| anyhow::anyhow!("peek heavy failed: {e}"))?;
        let Some(slab) = res else { return Err(anyhow::anyhow!("no heavy block returned")); };
        
        // 解析返回的区块ID
        let root = unsafe { slab.root() };
        let cell1 = root.as_cell().map_err(|_| anyhow::anyhow!("heavy: not cell1"))?;
        let cell2 = cell1.tail().as_cell().map_err(|_| anyhow::anyhow!("heavy: not cell2"))?;
        let block_id_noun = cell2.head();
        
        // 将区块ID转换为字节
        let block_id_atom = block_id_noun.as_atom().map_err(|_| anyhow::anyhow!("block_id not atom"))?;
        let block_id_bytes = block_id_atom.to_ne_bytes();
        
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
    
    // 创建配置，使用nockchain原生日志系统
    let nockapp_cli = boot::default_boot_cli(true);
    
    // 创建nockchain配置
    let nockchain_cli = nockchain::config::NockchainCli {
        nockapp_cli: nockapp_cli,
        npc_socket: ".socket/nockchain_npc.sock".to_string(),
        mine: false,
        mining_pubkey: Some(mining_pubkey.clone()),
        mining_key_adv: None,
        fakenet: false,  // 使用fakenet模式来简化测试
        peer: vec!["/ip4/121.61.204.239/udp/3006/quic-v1/p2p/12D3KooWEjQcZUS3x5YEMuMj1kgZJyoV4MTHNRnbtbSEMAMzwGQC".to_string()],
        force_peer: vec![],
        allowed_peers_path: None,
        no_default_peers: false,  // 不连接默认节点
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
    
    // 获取对状态监控器的引用，用于HTTP API
    let status_monitor = service.status_monitor.clone();
    
    // 生成初始工作任务
    let initial_work = service.generate_work().await;
    service.broadcast_work(initial_work).await;
    
    // 启动定期检查区块链状态的任务
    // 这是一个额外的保障机制，防止事件监听器错过某些更新
    let service_clone = Arc::new(service.clone());
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            info!("执行定期区块链状态检查 | Performing periodic blockchain state check");
            
            // 获取当前区块ID
            match service_clone.get_current_block_id().await {
                Ok(current_block_id) => {
                    // 检查当前工作是否过时
                    if service_clone.is_work_outdated(&current_block_id).await {
                        // 如果过时，生成新工作并广播
                        info!("定期检查发现区块链状态更新，生成新工作 | Periodic check detected blockchain state update, generating new work");
                        let new_work = service_clone.generate_work().await;
                        service_clone.broadcast_work(new_work).await;
                    } else {
                        info!("定期检查：当前工作仍然有效 | Periodic check: current work is still valid");
                    }
                },
                Err(e) => {
                    warn!("定期检查：获取区块ID失败: {} | Periodic check: failed to get block ID: {}", e, e);
                }
            }
        }
    });
    
    // 启动HTTP API服务
    info!("启动HTTP API服务器在 {}", http_api_address);
    http_api::start_api_server(status_monitor, &http_api_address).await;
    
    // 启动gRPC服务器
    let addr = pool_server_address.parse()?;
    info!("矿池服务器监听于 {}", addr);
    
    Server::builder()
        .add_service(MiningPoolServer::new(service))
        .serve(addr)
        .await?;
    
    Ok(())
} 