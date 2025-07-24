#!/bin/bash

echo "正在停止矿池控制中心进程..."
echo "Stopping pool server processes..."

# 停止pool-server程序
pkill -f "pool-server"
pkill -f "pool_server"
pkill -f "nockminer"

# 确认进程已停止
sleep 1
if pgrep -f "pool-server\|pool_server" > /dev/null; then
    echo "警告：部分进程可能未成功停止，尝试强制终止"
    echo "Warning: Some processes may not have stopped, trying to force kill"
    pkill -9 -f "pool-server"
    pkill -9 -f "pool_server"
fi

# 检查是否还有矿工进程
if pgrep -f "nockminer" > /dev/null; then
    echo "警告：发现矿工进程仍在运行，尝试强制终止"
    echo "Warning: Mining processes still running, trying to force kill"
    pkill -9 -f "nockminer"
fi

echo "矿池控制中心已停止"
echo "Pool server stopped"

# 显示最终确认
if pgrep -f "pool-server\|pool_server\|nockminer" > /dev/null; then
    echo "警告：仍有一些进程未能停止，可能需要手动终止"
    echo "Warning: Some processes could not be stopped, may need manual termination"
    ps aux | grep -i "pool\|nock\|miner" | grep -v grep
else
    echo "所有矿池相关进程已成功停止"
    echo "All pool-related processes successfully terminated"
fi