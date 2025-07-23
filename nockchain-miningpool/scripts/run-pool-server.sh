#!/bin/bash

# 设置环境变量以禁止跟踪系统初始化
export RUST_LOG=info
export TRACING_DISABLED=1

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

# 运行矿池服务器
echo "启动矿池服务器..."
echo "Starting pool server..."
cargo run --bin pool-server 