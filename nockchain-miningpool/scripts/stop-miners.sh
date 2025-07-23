#!/bin/bash
# stop-miners.sh

echo "正在停止所有矿工进程..."
pkill -f "miner.*127.0.0.1:7777"
echo "已停止"