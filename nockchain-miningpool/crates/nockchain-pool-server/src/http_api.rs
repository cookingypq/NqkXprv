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