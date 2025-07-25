#!/bin/bash
# 矿池服务器稳定性测试脚本

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 输出格式化函数
info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# 检查必要的命令是否存在
check_requirements() {
    info "检查系统要求..."
    
    if ! command -v cargo &> /dev/null; then
        error "未找到 cargo 命令，请安装 Rust 工具链"
        exit 1
    fi
    
    if ! command -v netstat &> /dev/null; then
        warn "未找到 netstat 命令，部分测试可能无法执行"
    fi
    
    success "系统检查通过"
}

# 构建矿池服务器和矿工客户端
build_binaries() {
    info "构建矿池服务器和矿工客户端..."
    
    # 进入项目根目录
    cd "$(dirname "$0")/.." || exit 1
    
    # 构建矿池服务器
    info "构建矿池服务器..."
    cargo build --release --bin pool-server
    if [ $? -ne 0 ]; then
        error "构建矿池服务器失败"
        exit 1
    fi
    
    # 构建矿工客户端
    info "构建矿工客户端..."
    cargo build --release --bin miner
    if [ $? -ne 0 ]; then
        error "构建矿工客户端失败"
        exit 1
    fi
    
    success "构建完成"
}

# 启动矿池服务器
start_pool_server() {
    info "启动矿池服务器..."
    
    # 检查端口是否已被占用
    if command -v netstat &> /dev/null; then
        if netstat -tuln | grep -q ":7777 "; then
            error "端口 7777 已被占用，请先关闭占用该端口的程序"
            exit 1
        fi
    fi
    
    # 设置环境变量
    export RUST_LOG=info
    export POOL_SERVER_ADDRESS=0.0.0.0:7777
    export HTTP_API_ADDRESS=0.0.0.0:8080
    export MINING_PUBKEY=test_pubkey_for_stability_test
    
    # 启动服务器（后台运行）
    ./target/release/pool-server > ./logs/pool-server-test.log 2>&1 &
    POOL_SERVER_PID=$!
    
    info "矿池服务器已启动，PID: $POOL_SERVER_PID"
    info "日志输出重定向到 ./logs/pool-server-test.log"
    
    # 等待服务器启动
    info "等待服务器就绪..."
    sleep 5
    
    # 检查服务器是否正常运行
    if ! ps -p $POOL_SERVER_PID > /dev/null; then
        error "矿池服务器启动失败，请检查日志"
        exit 1
    fi
    
    success "矿池服务器已就绪"
}

# 启动矿工客户端
start_miners() {
    info "启动矿工客户端..."
    
    # 设置环境变量
    export RUST_LOG=info
    
    # 启动多个矿工（后台运行）
    MINER_COUNT=${1:-3}  # 默认启动3个矿工
    MINER_PIDS=()
    
    for i in $(seq 1 $MINER_COUNT); do
        ./target/release/miner 127.0.0.1 -t 2 > ./logs/miner-$i-test.log 2>&1 &
        MINER_PID=$!
        MINER_PIDS+=($MINER_PID)
        info "矿工 $i 已启动，PID: $MINER_PID"
    done
    
    success "已启动 $MINER_COUNT 个矿工客户端"
}

# 测试连接稳定性
test_connection_stability() {
    info "测试连接稳定性..."
    
    # 等待一段时间，让系统稳定运行
    info "等待系统稳定运行 (10秒)..."
    sleep 10
    
    # 检查服务器和矿工进程是否仍在运行
    if ! ps -p $POOL_SERVER_PID > /dev/null; then
        error "矿池服务器已崩溃，测试失败"
        return 1
    fi
    
    for i in "${!MINER_PIDS[@]}"; do
        if ! ps -p ${MINER_PIDS[$i]} > /dev/null; then
            warn "矿工 $((i+1)) (PID: ${MINER_PIDS[$i]}) 已崩溃"
        fi
    done
    
    # 检查HTTP API是否可访问
    if command -v curl &> /dev/null; then
        info "测试HTTP API..."
        if curl -s http://localhost:8080/status > /dev/null; then
            success "HTTP API 可访问"
        else
            warn "HTTP API 不可访问"
        fi
    fi
    
    success "连接稳定性测试完成"
}

# 测试断线重连
test_reconnection() {
    info "测试断线重连..."
    
    # 如果没有矿工，跳过此测试
    if [ ${#MINER_PIDS[@]} -eq 0 ]; then
        warn "没有运行中的矿工，跳过断线重连测试"
        return 0
    fi
    
    # 选择第一个矿工进行断线测试
    TEST_MINER_PID=${MINER_PIDS[0]}
    info "选择矿工 1 (PID: $TEST_MINER_PID) 进行断线测试"
    
    # 终止矿工进程
    kill -TERM $TEST_MINER_PID
    info "已终止矿工进程，等待5秒..."
    sleep 5
    
    # 重启矿工
    ./target/release/miner 127.0.0.1 -t 2 > ./logs/miner-reconnect-test.log 2>&1 &
    NEW_MINER_PID=$!
    MINER_PIDS[0]=$NEW_MINER_PID
    info "矿工已重启，新PID: $NEW_MINER_PID"
    
    # 等待重连
    info "等待重连 (10秒)..."
    sleep 10
    
    # 检查重启的矿工是否仍在运行
    if ! ps -p $NEW_MINER_PID > /dev/null; then
        error "重连的矿工已崩溃，测试失败"
        return 1
    fi
    
    success "断线重连测试完成"
}

# 清理资源
cleanup() {
    info "清理资源..."
    
    # 终止所有矿工进程
    for pid in "${MINER_PIDS[@]}"; do
        if ps -p $pid > /dev/null; then
            kill -TERM $pid
            info "已终止矿工进程 (PID: $pid)"
        fi
    done
    
    # 终止矿池服务器进程
    if [ -n "$POOL_SERVER_PID" ] && ps -p $POOL_SERVER_PID > /dev/null; then
        kill -TERM $POOL_SERVER_PID
        info "已终止矿池服务器进程 (PID: $POOL_SERVER_PID)"
    fi
    
    success "资源清理完成"
}

# 主函数
main() {
    info "=== 矿池服务器稳定性测试开始 ==="
    
    # 确保logs目录存在
    mkdir -p ./logs
    
    # 注册清理函数
    trap cleanup EXIT
    
    # 执行测试步骤
    check_requirements
    build_binaries
    start_pool_server
    start_miners 3  # 启动3个矿工
    test_connection_stability
    test_reconnection
    
    success "=== 矿池服务器稳定性测试完成 ==="
}

# 执行主函数
main "$@" 