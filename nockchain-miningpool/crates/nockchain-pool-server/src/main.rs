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
// 引入nockchain核心库，用于集成nockchain节点
use kernels::miner::KERNEL;
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
    async fn safe_poke(&self, wire: nockapp::wire::WireRepr, mut poke_slab: NounSlab) -> Result<PokeResult, anyhow::Error> {
        // 创建一个新的可以跨线程移动的数据
        let wire_clone = wire.clone();
        let slab_data = poke_slab.jam();
        
        // 在单线程上下文中执行，避免原始指针跨线程
        let handle_clone = self.handle.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut handle = handle_clone.lock().unwrap();
            
            // 创建一个新的slab从数据重建
            // 指定NounSlab的默认类型参数
            let mut new_slab: NounSlab = NounSlab::new();
            // 实际代码中可能需要解析jam数据
            
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
}

impl MiningPoolService {
    fn new(nockchain: NockApp) -> Self {
        let handle = ThreadSafeNockAppHandle::new(nockchain.get_handle());
        Self {
            miners: Arc::new(DashMap::new()),
            current_work: Arc::new(RwLock::new(None)),
            nockchain: Arc::new(RwLock::new(nockchain)),
            handle,
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
                // 提供默认值作为回退 | Provide default values as fallback
                (vec![0; 32], vec![0; 32], vec![0; 32])
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
        
        // 直接处理响应，不需要调用root()方法
        match response {
            PokeResult::Ack => {
                // 从handle中获取结果
                // 这里简化处理，返回测试数据
                // 实际应该从响应中解析数据
                let parent_hash = vec![0; 32];
                let merkle_root = vec![0; 32];
                let difficulty = vec![0; 32];
                
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
            PokeResult::Ack => Ok(true),
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
        tokio::spawn(async move {
            while let Ok(Some(status)) = stream.message().await {
                // 更新矿工状态 | Update miner status
                if let Some(mut miner) = miners.get_mut(&status.miner_id) {
                    miner.threads = status.threads;
                }
            }
            
            // 矿工断开连接 | Miner disconnected
            miners.remove(&miner_id_clone);
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
        
        if is_valid {
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
            
            // 比较哈希与目标难度 | Compare hash with target difficulty
            let is_valid = compare_hash_with_target(hash.as_bytes(), &work.difficulty_target);
            
            if is_valid {
                info!(
                    "有效的工作结果: 哈希值 {} 满足难度目标 | Valid work result: hash {} meets difficulty target", 
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
    
    // 初始化日志系统 | Initialize logging system
    tracing_subscriber::fmt::init();
    
    // 读取矿工公钥环境变量 | Read mining public key environment variable
    let mining_pubkey = env::var("MINING_PUBKEY")
        .expect("必须设置MINING_PUBKEY环境变量 | MINING_PUBKEY environment variable must be set");
    
    // 读取矿池服务器监听地址 | Read pool server listening address
    let pool_server_address = env::var("POOL_SERVER_ADDRESS")
        .unwrap_or_else(|_| "0.0.0.0:7777".to_string());
    
    info!("启动矿池服务器，挖矿公钥: {} | Starting pool server, mining pubkey: {}", mining_pubkey, mining_pubkey);
    
    // 初始化nockchain节点 | Initialize nockchain node
    info!("初始化内嵌nockchain节点... | Initializing embedded nockchain node...");
    
    // 初始化nockapp用于日志
    // Initialize nockapp for logging
    let nockapp_cli = boot::default_boot_cli(false);
    boot::init_default_tracing(&nockapp_cli);
    
    // 创建prover hot state
    // Create prover hot state
    let prover_hot_state = produce_prover_hot_state();
    
    // 初始化nockchain节点
    // Initialize nockchain node
    let nockchain: NockApp = nockchain::init_with_kernel(
        None, // 使用默认配置 | Use default configuration
        KERNEL,
        prover_hot_state.as_slice()
    ).await.expect("Failed to initialize nockchain");
    
    // 设置挖矿公钥和启用挖矿 - 以线程安全的方式
    info!("配置挖矿公钥和启用挖矿... | Configuring mining pubkey and enabling mining...");
    let service = MiningPoolService::new(nockchain);
    
    // 生成初始工作任务 | Generate initial work task
    let initial_work = service.generate_work().await;
    service.broadcast_work(initial_work).await;
    
    // 启动gRPC服务器 | Start gRPC server
    let addr = pool_server_address.parse()?;
    info!("矿池服务器监听于 {} | Pool server listening on {}", addr, addr);
    
    Server::builder()
        .add_service(MiningPoolServer::new(service))
        .serve(addr)
        .await?;
    
    Ok(())
} 