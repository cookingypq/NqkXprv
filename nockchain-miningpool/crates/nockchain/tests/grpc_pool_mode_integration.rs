//! Integration test for miner gRPC pool mode

use std::time::Duration;
use tonic::transport::Channel;
use tonic::Request;
use tokio::time::timeout;

// 导入gRPC生成的代码
pub mod pool {
    tonic::include_proto!("pool");
}
use pool::{mining_pool_client::MiningPoolClient, MinerStatus, WorkResult};

const SERVER_ADDR: &str = "http://127.0.0.1:7777";

#[tokio::test]
#[ignore]
async fn test_miner_grpc_pool_mode_workflow() {
    // 1. 连接到pool-server
    let mut client = MiningPoolClient::connect(SERVER_ADDR.to_string())
        .await
        .expect("Failed to connect to pool-server");

    // 2. 向Subscribe发起stream，发送MinerStatus
    let (mut tx, rx) = tokio::sync::mpsc::channel(8);
    let outbound = async_stream::stream! {
        // 发送一次心跳
        yield MinerStatus {
            miner_id: "test_miner_1".to_string(),
            threads: 2,
        };
        // 可扩展：定期发送心跳
    };
    let response = client.subscribe(Request::new(outbound)).await.expect("Subscribe failed");
    let mut work_stream = response.into_inner();

    // 3. 接收work order
    let work_order = timeout(Duration::from_secs(3), work_stream.message())
        .await
        .expect("Timeout waiting for work order")
        .expect("Stream closed")
        .expect("No work order received");
    assert!(!work_order.work_id.is_empty(), "Work order should have id");

    // 4. 提交work result（模拟无效nonce）
    let result = WorkResult {
        work_id: work_order.work_id.clone(),
        nonce: vec![0, 0, 0, 0],
        miner_id: "test_miner_1".to_string(),
    };
    let ack = client.submit_work(Request::new(result)).await.expect("SubmitWork failed").into_inner();
    assert!(!ack.success, "Invalid nonce should not be accepted");

    // 5. 可扩展：断开重连、异常场景
} 