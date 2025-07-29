//! End-to-end gRPC integration test for pool-server

use std::time::Duration;
use tonic::transport::Channel;
use tonic::Request;
use tokio::time::timeout;

pub mod pool {
    tonic::include_proto!("pool");
}
use pool::{mining_pool_client::MiningPoolClient, MinerStatus, WorkResult};

const SERVER_ADDR: &str = "http://127.0.0.1:7777";

async fn connect_and_subscribe(miner_id: &str, threads: u32) -> MiningPoolClient<Channel> {
    let mut client = MiningPoolClient::connect(SERVER_ADDR.to_string())
        .await
        .expect("Failed to connect to pool-server");
    let outbound = async_stream::stream! {
        yield MinerStatus {
            miner_id: miner_id.to_string(),
            threads,
        };
    };
    let response = client.subscribe(Request::new(outbound)).await.expect("Subscribe failed");
    response.into_inner();
    client
}

#[tokio::test]
#[ignore]
async fn test_multiple_miners_end_to_end() {
    // 1. 启动多个miner连接
    let mut clients = vec![];
    for i in 0..3 {
        let miner_id = format!("test_miner_{}", i);
        let client = MiningPoolClient::connect(SERVER_ADDR.to_string())
            .await
            .expect("Failed to connect to pool-server");
        let outbound = async_stream::stream! {
            yield MinerStatus {
                miner_id: miner_id.clone(),
                threads: 2,
            };
        };
        let response = client.subscribe(Request::new(outbound)).await.expect("Subscribe failed");
        clients.push((miner_id, client, response.into_inner()));
    }

    // 2. 每个miner接收work order
    for (miner_id, client, mut work_stream) in &mut clients {
        let work_order = timeout(Duration::from_secs(3), work_stream.message())
            .await
            .expect("Timeout waiting for work order")
            .expect("Stream closed")
            .expect("No work order received");
        assert!(!work_order.work_id.is_empty(), "Work order should have id");

        // 3. 提交work result（模拟无效nonce）
        let result = WorkResult {
            work_id: work_order.work_id.clone(),
            nonce: vec![0, 0, 0, 0],
            miner_id: miner_id.clone(),
        };
        let ack = client.submit_work(Request::new(result)).await.expect("SubmitWork failed").into_inner();
        assert!(!ack.success, "Invalid nonce should not be accepted");
    }

    // 4. 模拟断连重连
    let (miner_id, mut client, mut work_stream) = clients.pop().unwrap();
    drop(work_stream); // 模拟断开
    tokio::time::sleep(Duration::from_millis(500)).await;
    // 重连
    let outbound = async_stream::stream! {
        yield MinerStatus {
            miner_id: miner_id.clone(),
            threads: 2,
        };
    };
    let response = client.subscribe(Request::new(outbound)).await.expect("Reconnect Subscribe failed");
    let mut work_stream = response.into_inner();
    let work_order = timeout(Duration::from_secs(3), work_stream.message())
        .await
        .expect("Timeout waiting for work order after reconnect")
        .expect("Stream closed after reconnect")
        .expect("No work order received after reconnect");
    assert!(!work_order.work_id.is_empty(), "Work order should have id after reconnect");

    // 5. 异常场景：提交无效work_id
    let result = WorkResult {
        work_id: "invalid_id".to_string(),
        nonce: vec![1, 2, 3, 4],
        miner_id: miner_id.clone(),
    };
    let ack = client.submit_work(Request::new(result)).await.expect("SubmitWork failed").into_inner();
    assert!(!ack.success, "Invalid work_id should not be accepted");
} 