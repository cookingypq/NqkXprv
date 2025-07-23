#!/bin/bash
# 测试挖矿难度设置是否正确的脚本

echo "=== 验证挖矿难度设置脚本 ==="
echo "This script validates mining difficulty settings"

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# 检查必要文件
if [ ! -f "$ROOT_DIR/target/debug/pool-server" ]; then
    echo "正在构建池服务器..."
    cd "$ROOT_DIR" && cargo build --bin pool-server
    if [ $? -ne 0 ]; then
        echo "构建失败！"
        exit 1
    fi
fi

# 设置测试环境
export RUST_LOG="debug,info"
export MINING_PUBKEY="3geooCU86ng5CEdXKqmmb37nScanFH7K782VRJHwnNegFmkEf7mdM2pftFE8t8tT1kBiJVHNzmNCfzAdD8ABJUo52GGfTLDbK9wj7Dh8Y6KEuqgL4MngbfEVW456vTanW2ij"
export POOL_SERVER_ADDRESS="127.0.0.1:7777"
export HTTP_API_ADDRESS="127.0.0.1:8080"

# 创建临时日志文件
LOG_FILE=$(mktemp)
echo "日志文件: $LOG_FILE"

# 启动服务器，macOS兼容方式
echo "启动池服务器..."
"$ROOT_DIR/target/debug/pool-server" > "$LOG_FILE" 2>&1 &
SERVER_PID=$!

# 等待几秒钟让服务器初始化
echo "等待服务器初始化 (5秒)..."
sleep 5

# 结束服务器进程
echo "停止服务器进程..."
kill $SERVER_PID 2>/dev/null || true

# 检查日志中是否包含难度设置信息
echo "检查难度设置..."
if grep -q "设置挖矿难度目标\|难度目标" "$LOG_FILE"; then
    echo "✅ 成功: 找到难度设置日志"
    grep -E "设置挖矿难度目标|难度目标" "$LOG_FILE"
else
    echo "❌ 失败: 未找到难度设置日志"
    echo "日志内容前20行:"
    head -20 "$LOG_FILE"
fi

# 检查是否包含全0的难度目标
if grep -q "difficulty.*00000000000000000000000000000000000000000000000000000000000000" "$LOG_FILE"; then
    echo "❌ 失败: 发现全0难度目标！"
    grep -E "difficulty|难度" "$LOG_FILE"
    exit 1
else
    echo "✅ 成功: 未发现全0难度目标"
fi

# 在日志中查找工作ID和难度目标
if grep -q "current_work_id\|当前难度目标\|工作任务" "$LOG_FILE"; then
    echo "✅ 找到工作ID或难度目标相关日志:"
    grep -E "current_work_id|当前难度目标|工作任务" "$LOG_FILE" | head -5
else
    echo "⚠️ 警告: 未找到工作ID或难度相关日志"
fi

echo "测试完成，日志文件: $LOG_FILE"
echo "=== 测试完成 ===" 