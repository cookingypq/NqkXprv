use std::error::Error;

use anyhow::Result;
use clap::Parser;
use kernels::dumb::KERNEL;
use nockapp::kernel::boot;
use nockapp::NockApp;
use zkvm_jetpack::hot::produce_prover_hot_state;

/// 定义新的命令行参数结构
#[derive(Parser, Debug)]
#[command(name = "miner", version, about)]
struct MinerCli {
    /// 矿池服务器的地址和端口，例如： "http://127.0.0.1:7777"
    /// 如果提供了此参数，程序将以矿池客户端模式运行。
    pool_server: Option<String>,

    /// 在矿池模式下，挖矿使用的线程数。
    /// 如果不提供，将使用CPU核心数。
    #[arg(short, long)]
    threads: Option<u32>,
    
    /// 当不提供 pool-server 时，使用标准的 nockchain 启动参数。
    #[command(flatten)]
    nockchain_args: nockchain::config::NockchainCli,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    nockvm::check_endian();
    let cli = MinerCli::parse();
    
    // 初始化日志系统
    boot::init_default_tracing(&cli.nockchain_args.nockapp_cli);

    if let Some(server_address) = cli.pool_server {
        // 运行矿池客户端模式
        tracing::info!("以矿池模式启动，连接到 {}", server_address);
        run_pool_mode(server_address, cli.threads).await?;
    } else {
        // 运行独立的SOLO矿工模式
        tracing::info!("以SOLO矿工模式启动");
        run_solo_mode(cli.nockchain_args).await?;
    }

    Ok(())
}

/// 矿池模式 - 连接到矿池服务器
async fn run_pool_mode(server_address: String, threads_opt: Option<u32>) -> Result<(), Box<dyn Error>> {
    use nockchain::pool_client::{PoolClient, PoolClientConfig};
    use uuid::Uuid;

    // 修复类型不匹配错误：将usize转换为u32
    let threads = threads_opt.unwrap_or_else(|| num_cpus::get_physical() as u32);
    let miner_id = Uuid::new_v4().to_string();

    tracing::info!("矿工ID: {}, 使用线程数: {}", miner_id, threads);

    // 补充缺失的字段
    let config = PoolClientConfig {
        server_address,
        threads,
        miner_id,
        backup_servers: vec![], // 添加备用服务器列表
        connection_retry_attempts: 3, // 添加重试次数
        connection_retry_delay_ms: 1000, // 添加重试延迟
        connection_timeout_ms: 30000, // 修正字段名称：会话超时
        request_timeout_ms: 10000, // 添加请求超时
    };

    let client = PoolClient::new(config).await?;
    client.run().await?;
    
    Ok(())
}

/// SOLO模式 - 运行完整的nockchain节点
async fn run_solo_mode(cli: nockchain::config::NockchainCli) -> Result<(), Box<dyn Error>> {
    let prover_hot_state = produce_prover_hot_state();
    let mut nockchain: NockApp =
        nockchain::init_with_kernel(Some(cli), KERNEL, prover_hot_state.as_slice()).await?;
    nockchain.run().await?;
    Ok(())
}
