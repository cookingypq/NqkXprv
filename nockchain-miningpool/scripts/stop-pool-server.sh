echo "正在停止矿池控制中心进程..."
echo "Stopping pool server processes..."

# 停止pool-server程序
pkill -f "pool-server"

# 确认进程已停止
sleep 1
if pgrep -f "pool-server" > /dev/null; then
    echo "警告：部分进程可能未成功停止，尝试强制终止"
    echo "Warning: Some processes may not have stopped, trying force termination"
    pkill -9 -f "pool-server"
fi

echo "矿池控制中心已停止"
echo "Pool server stopped"