use std::sync::Arc;
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::Serialize;
use tracing::{info, error};
use tower_http::cors::{CorsLayer, Any};
use tokio::net::TcpListener;
use crate::status_monitor::{StatusMonitor, PoolStatistics};

// API响应格式
#[derive(Serialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

// 请求处理状态
#[derive(Clone)]
pub struct ApiState {
    pub status_monitor: Arc<StatusMonitor>,
}

// 获取矿池状态
async fn get_pool_status(
    State(state): State<ApiState>,
) -> (StatusCode, Json<ApiResponse<PoolStatistics>>) {
    match state.status_monitor.get_statistics().await {
        stats => {
            (
                StatusCode::OK,
                Json(ApiResponse {
                    success: true,
                    data: Some(stats),
                    error: None,
                }),
            )
        }
    }
}

// 获取简化状态信息
#[derive(Serialize)]
struct BasicStatus {
    miners: usize,
    hashrate: u64,
    blocks: u64,
    current_height: u64,
}

async fn get_basic_status(
    State(state): State<ApiState>,
) -> (StatusCode, Json<ApiResponse<BasicStatus>>) {
    let stats = state.status_monitor.get_statistics().await;
    let basic = BasicStatus {
        miners: stats.connected_miners,
        hashrate: stats.estimated_hashrate,
        blocks: stats.blocks_accepted,
        current_height: stats.current_block_height,
    };
    
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            data: Some(basic),
            error: None,
        }),
    )
}

// 同步状态信息
#[derive(Serialize)]
struct SyncStatus {
    current_block_height: u64,
    network_block_height: Option<u64>,
    sync_percentage: f64,
    sync_status: String,
    latest_block_hash: String,
    last_block_time: Option<String>,
    remaining_blocks: Option<u64>,
    estimated_time_remaining: Option<String>,
}

// 获取区块链同步状态
async fn get_sync_status(
    State(state): State<ApiState>,
) -> (StatusCode, Json<ApiResponse<SyncStatus>>) {
    let stats = state.status_monitor.get_statistics().await;
    
    // 计算剩余区块数
    let remaining_blocks = match stats.network_latest_block_height {
        Some(network_height) if network_height > stats.current_block_height => {
            Some(network_height - stats.current_block_height)
        },
        _ => None
    };
    
    // 格式化最后区块时间
    let last_block_time = stats.last_block_time.map(|dt| dt.to_rfc3339());
    
    // 简单估算剩余时间（假设每个区块同步需要2秒）
    let estimated_time_remaining = remaining_blocks.map(|blocks| {
        let seconds = blocks * 2; // 简单估算，每个区块2秒
        if seconds < 60 {
            format!("{}秒", seconds)
        } else if seconds < 3600 {
            format!("{}分钟", seconds / 60)
        } else {
            format!("{}小时{}分钟", seconds / 3600, (seconds % 3600) / 60)
        }
    });
    
    let sync_status = SyncStatus {
        current_block_height: stats.current_block_height,
        network_block_height: stats.network_latest_block_height,
        sync_percentage: stats.sync_percentage,
        sync_status: stats.sync_status,
        latest_block_hash: stats.latest_block_hash,
        last_block_time,
        remaining_blocks,
        estimated_time_remaining,
    };
    
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            data: Some(sync_status),
            error: None,
        }),
    )
}

// 健康检查端点
async fn health_check() -> StatusCode {
    StatusCode::OK
}

// 启动HTTP API服务器
pub async fn start_api_server(status_monitor: Arc<StatusMonitor>, bind_address: &str) {
    info!("启动HTTP API服务器在 {}", bind_address);

    // 创建共享状态
    let state = ApiState {
        status_monitor,
    };

    // 配置CORS - 适配tower-http 0.5版本
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // 构建路由
    let app = Router::new()
        .route("/api/status", get(get_pool_status))
        .route("/api/basic", get(get_basic_status))
        .route("/api/sync", get(get_sync_status))
        .route("/health", get(health_check))
        .with_state(state)
        .layer(cors); // 先设置状态，后添加中间件层

    // 绑定TCP监听器
    let listener = match TcpListener::bind(bind_address).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("无法绑定HTTP服务器到 {}: {}", bind_address, e);
            return;
        }
    };

    // 启动服务器 - 使用axum 0.7版本的API
    info!("HTTP API服务器已启动在 {}", bind_address);
    
    // 启动HTTP服务器并使用tokio::spawn异步执行
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            error!("HTTP API服务器错误: {}", e);
        }
    });
}