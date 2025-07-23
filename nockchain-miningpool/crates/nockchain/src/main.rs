use std::error::Error;

use anyhow::{Context, Result};
use clap::{Parser, arg};
use kernels::dumb::KERNEL;
use nockapp::kernel::boot;
use nockapp::NockApp;
use uuid::Uuid;
use zkvm_jetpack::hot::produce_prover_hot_state;

// 当启用jemalloc特性时，使用jemalloc获得更稳定的内存分配
// When enabled, use jemalloc for more stable memory allocation
#[cfg(feature = "jemalloc")]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// 为矿池客户端定义命令行参数
// Define command line arguments for pool client
#[derive(Parser, Debug)]
#[command(name = "miner")]
struct MinerCli {
    /// 矿池服务器地址 (如果提供，则以矿池模式运行)
    /// Pool server address (if provided, run in pool mode)
    #[arg(help = "矿池服务器地址。如果提供，则以矿池客户端模式运行。否则，以SOLO矿工模式运行。| Pool server address. If provided, run in pool client mode. Otherwise, run as a SOLO miner.")]
    pool_server: Option<String>,

    /// 挖矿使用的线程数
    /// Number of threads to use for mining
    #[arg(short, long, help = "挖矿使用的线程数 | Number of threads to use for mining")]
    threads: Option<u32>,

    /// 标准的nockchain参数（当以SOLO模式运行时使用）
    /// Standard nockchain arguments (used when running in SOLO mode)
    #[command(flatten)]
    nockchain_args: nockchain::NockchainCli,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 检查系统字节序
    // Check system endianness
    nockvm::check_endian();
    
    // 解析命令行参数
    // Parse command line arguments
    let cli = MinerCli::parse();

    // 初始化日志系统
    // Initialize logging system
    boot::init_default_tracing(&cli.nockchain_args.nockapp_cli);

    // 检查是否提供了矿池服务器地址
    // Check if pool server address is provided
    if let Some(server_address) = cli.pool_server {
        // 以矿池模式运行
        // Run in pool mode
        run_pool_mode(server_address, cli.threads).await?;
    } else {
        // 以SOLO模式运行
        // Run in SOLO mode
        run_solo_mode(cli.nockchain_args).await?;
    }
    
    Ok(())
}

/// 矿池模式 - 连接到矿池服务器
/// Pool mode - Connect to pool server
#[cfg(feature = "pool_client")]
async fn run_pool_mode(server_address: String, threads_opt: Option<u32>) -> Result<(), Box<dyn Error>> {
    use nockchain::pool_client::{PoolClient, PoolClientConfig};
    use tracing::{info, error};

    info!("以矿池模式启动 | Starting in pool mode");
    
    // 确定线程数，如果未指定则使用系统CPU核心数减1
    // Determine thread count, if not specified use system CPU cores minus 1
    let threads = threads_opt.unwrap_or_else(|| {
        let num_cpus = num_cpus::get() as u32;
        if num_cpus > 1 {
            num_cpus - 1
        } else {
            1
        }
    });

    // 生成唯一矿工ID
    // Generate unique miner ID
    let miner_id = format!("miner-{}", Uuid::new_v4().to_string().split('-').next().unwrap_or("unknown"));
    
    info!("矿工ID: {}, 线程数: {} | Miner ID: {}, Thread count: {}", miner_id, threads, miner_id, threads);

    // 创建矿池客户端配置
    // Create pool client configuration
    let config = PoolClientConfig {
        server_address,
        threads,
        miner_id,
    };

    // 创建并启动矿池客户端
    // Create and start pool client
    match PoolClient::new(config).await {
        Ok(client) => {
            if let Err(e) = client.run().await {
                error!("矿池客户端运行时出错: {} | Error running pool client: {}", e, e);
                return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        }
        Err(e) => {
            error!("创建矿池客户端时出错: {} | Error creating pool client: {}", e, e);
            return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
        }
    }
    
    Ok(())
}

/// 当未启用pool_client特性时的占位实现
/// Placeholder implementation when pool_client feature is not enabled
#[cfg(not(feature = "pool_client"))]
async fn run_pool_mode(_server_address: String, _threads_opt: Option<u32>) -> Result<(), Box<dyn Error>> {
    eprintln!("错误：矿池模式未启用，请使用 --features pool_client 重新编译 | Error: Pool mode not enabled, please recompile with --features pool_client");
    Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "矿池模式未启用 | Pool mode not enabled")))
}

/// SOLO模式 - 运行完整的nockchain节点
/// SOLO mode - Run full nockchain node
async fn run_solo_mode(cli: nockchain::NockchainCli) -> Result<(), Box<dyn Error>> {
    use tracing::info;
    
    info!("以SOLO模式启动 | Starting in SOLO mode");
    
    let prover_hot_state = produce_prover_hot_state();
    let mut nockchain: NockApp =
        nockchain::init_with_kernel(Some(cli), KERNEL, prover_hot_state.as_slice()).await?;
    nockchain.run().await?;
    
    Ok(())
}
