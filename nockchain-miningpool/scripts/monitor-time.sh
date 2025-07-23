#!/bin/bash
# monitor-time.sh

echo "开始监控nonce计算时间..."
echo "Starting monitoring nonce calculation time..."

# 直接从标准输出打印
while true; do
  echo "====== $(date) ======"
  
  # 查找所有miner进程的PID
  MINER_PIDS=$(ps -ef | grep 'miner 127.0.0.1' | grep -v grep | awk '{print $2}')
  
  # 计算进程数
  MINER_COUNT=$(echo "$MINER_PIDS" | wc -l | xargs)
  echo "找到 $MINER_COUNT 个矿工进程"
  echo "Found $MINER_COUNT miner processes"
  
  # 获取总体哈希率估算
  TOTAL_HASHRATE=0
  for pid in $MINER_PIDS; do
    # 根据CPU使用率粗略估计哈希率
    CPU_USAGE=$(ps -p $pid -o %cpu | tail -1 | xargs)
    # 假设每1%CPU约等于10000 H/s (这只是一个粗略估算)
    ESTIMATED_HASHRATE=$(echo "$CPU_USAGE * 10000" | bc)
    TOTAL_HASHRATE=$(echo "$TOTAL_HASHRATE + $ESTIMATED_HASHRATE" | bc)
    echo "矿工进程 $pid: 估计哈希率 ~$ESTIMATED_HASHRATE H/s (CPU: $CPU_USAGE%)"
    echo "Miner process $pid: estimated hashrate ~$ESTIMATED_HASHRATE H/s (CPU: $CPU_USAGE%)"
  done
  
  # 如果有总哈希率，计算平均nonce时间
  if [ "$TOTAL_HASHRATE" != "0" ]; then
    AVG_NONCE_TIME=$(echo "scale=9; 1 / $TOTAL_HASHRATE * 1000000" | bc)
    echo "总估计哈希率: ~$TOTAL_HASHRATE H/s"
    echo "Total estimated hashrate: ~$TOTAL_HASHRATE H/s"
    echo "平均每个nonce计算时间: ~$AVG_NONCE_TIME 微秒"
    echo "Average time per nonce: ~$AVG_NONCE_TIME microseconds"
  else
    echo "无法估算哈希率，请确保矿工进程正在运行"
    echo "Cannot estimate hashrate, please ensure miner processes are running"
  fi
  
  echo ""  # 添加空行
  sleep 5  # 每5秒更新一次
done