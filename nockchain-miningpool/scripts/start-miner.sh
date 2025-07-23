#!/bin/bash

# 启动矿工客户端的脚本
# Script to start the miner client

# 默认配置
# Default configuration
POOL_SERVER=""
THREADS=$(( $(sysctl -n hw.logicalcpu) - 1 ))
RUST_LOG=${RUST_LOG:-"info"}
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# 如果系统只有一个CPU核心，使用它
# If system has only one CPU core, use it
if [ $THREADS -lt 1 ]; then
    THREADS=1
fi

# 显示帮助信息
# Display help information
show_help() {
    echo "使用方法: $0 <矿池服务器地址> [选项]"
    echo "Usage: $0 <pool server address> [options]"
    echo ""
    echo "参数 | Parameters:"
    echo "  <矿池服务器地址>   矿池服务器的IP地址或主机名 | Pool server IP address or hostname"
    echo "                    例如 | Example: 192.168.1.100 或 | or my-server.local"
    echo ""
    echo "选项 | Options:"
    echo "  -t, --threads N    使用的线程数 | Number of threads to use (default: $THREADS)"
    echo "  -l, --log LEVEL    设置日志级别 | Set log level (trace, debug, info, warn, error)"
    echo "  -h, --help         显示此帮助信息 | Show this help information"
    echo ""
    echo "示例 | Example:"
    echo "  $0 192.168.1.100 --threads 8"
    exit 0
}

# 检查参数数量
# Check number of arguments
if [ $# -lt 1 ]; then
    echo "错误: 缺少矿池服务器地址 | Error: Missing pool server address"
    show_help
fi

# 获取第一个参数作为服务器地址
# Get first argument as server address
POOL_SERVER="$1"
shift

# 解析剩余命令行参数
# Parse remaining command line arguments
while [[ $# -gt 0 ]]; do
    key="$1"
    case $key in
        -t|--threads)
            THREADS="$2"
            shift
            shift
            ;;
        -l|--log)
            RUST_LOG="$2"
            shift
            shift
            ;;
        -h|--help)
            show_help
            ;;
        *)
            echo "未知选项: $1"
            echo "Unknown option: $1"
            show_help
            ;;
    esac
done

# 验证服务器地址
# Validate server address
if [ -z "$POOL_SERVER" ]; then
    echo "错误: 无效的矿池服务器地址 | Error: Invalid pool server address"
    show_help
fi

# 验证线程数
# Validate thread count
if ! [[ "$THREADS" =~ ^[0-9]+$ ]]; then
    echo "错误: 线程数必须为正整数 | Error: Thread count must be a positive integer"
    exit 1
fi

# 设置环境变量
# Set environment variables
export RUST_LOG

echo "启动矿工客户端 | Starting miner client"
echo "矿池服务器 | Pool server: $POOL_SERVER"
echo "线程数 | Thread count: $THREADS"
echo "日志级别 | Log level: $RUST_LOG"

# 启动矿工客户端
# Start miner client
if [ -f "$ROOT_DIR/target/release/miner" ]; then
    "$ROOT_DIR/target/release/miner" "$POOL_SERVER" --threads "$THREADS"
else
    echo "错误: 找不到miner可执行文件。请先运行'make release'来构建它"
    echo "Error: Cannot find miner executable. Please run 'make release' to build it first"
    exit 1
fi