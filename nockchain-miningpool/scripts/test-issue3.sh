#!/bin/bash

# 设置详细日志级别以便调试
export RUST_LOG=info,nockchain=debug,nockchain_libp2p_io=debug,libp2p=debug,libp2p_quic=debug
export MINIMAL_LOG_FORMAT=true

# 确保加载环境变量
if [ -f .env ]; then
  source .env
fi

# 检查必要的环境变量
if [ -z "$MINING_PUBKEY" ]; then
  echo "错误: MINING_PUBKEY 环境变量未设置。请在 .env 文件中设置或直接导出。"
  echo "Error: MINING_PUBKEY environment variable not set. Set it in .env file or export directly."
  exit 1
fi

# 创建日志目录
mkdir -p logs

# 备份之前的日志
if [ -f logs/pool-server-issue3-test.log ]; then
  mv logs/pool-server-issue3-test.log logs/pool-server-issue3-test.log.bak
fi

echo "启动池服务器测试 Issue #3：协议不匹配问题..."
echo "Starting pool server to test Issue #3: Protocol mismatch problem..."
echo "详细日志将写入 logs/pool-server-issue3-test.log"
echo "Detailed logs will be written to logs/pool-server-issue3-test.log"

# 运行矿池服务器，指定只使用QUIC协议
cargo run --bin pool-server 2>&1 | tee logs/pool-server-issue3-test.log 