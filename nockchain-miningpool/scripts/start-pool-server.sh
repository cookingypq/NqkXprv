#!/bin/bash

# 启动矿池服务器的脚本
# Script to start the pool server

# 默认配置
# Default configuration
MINING_PUBKEY=${MINING_PUBKEY:-""}
RUST_LOG=${RUST_LOG:-"info"}
POOL_SERVER_ADDRESS=${POOL_SERVER_ADDRESS:-"0.0.0.0:7777"}
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# 显示帮助信息
# Display help information
show_help() {
    echo "使用方法: $0 [选项]"
    echo "Usage: $0 [options]"
    echo ""
    echo "选项 | Options:"
    echo "  -p, --pubkey KEY    设置挖矿公钥 | Set mining public key"
    echo "  -a, --address ADDR  设置服务器地址和端口 | Set server address and port (default: 0.0.0.0:7777)"
    echo "  -l, --log LEVEL     设置日志级别 | Set log level (trace, debug, info, warn, error)"
    echo "  -h, --help          显示此帮助信息 | Show this help information"
    echo ""
    echo "示例 | Example:"
    echo "  $0 --pubkey YOUR_PUBLIC_KEY --log debug"
    echo ""
    echo "注意: 你也可以使用.env文件设置这些变量 | Note: You can also use a .env file to set these variables"
    exit 0
}

# 解析命令行参数
# Parse command line arguments
while [[ $# -gt 0 ]]; do
    key="$1"
    case $key in
        -p|--pubkey)
            MINING_PUBKEY="$2"
            shift
            shift
            ;;
        -a|--address)
            POOL_SERVER_ADDRESS="$2"
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

# 检查是否提供了挖矿公钥
# Check if mining public key is provided
if [ -z "$MINING_PUBKEY" ]; then
    # 检查是否存在.env文件
    # Check if .env file exists
    if [ -f "$ROOT_DIR/.env" ]; then
        echo "从.env文件加载配置 | Loading configuration from .env file"
        source "$ROOT_DIR/.env"
    fi
    
    # 再次检查是否有挖矿公钥
    # Check again if mining public key exists
    if [ -z "$MINING_PUBKEY" ]; then
        echo "错误: 缺少挖矿公钥。请使用--pubkey参数或在.env文件中设置MINING_PUBKEY"
        echo "Error: Missing mining public key. Please use --pubkey option or set MINING_PUBKEY in .env file"
        exit 1
    fi
fi

# 设置环境变量
# Set environment variables
export MINING_PUBKEY
export RUST_LOG
export POOL_SERVER_ADDRESS

echo "启动矿池服务器 | Starting pool server"
echo "挖矿公钥 | Mining public key: $MINING_PUBKEY"
echo "监听地址 | Listening address: $POOL_SERVER_ADDRESS"
echo "日志级别 | Log level: $RUST_LOG"

# 启动矿池服务器
# Start pool server
if [ -f "$ROOT_DIR/target/release/pool-server" ]; then
    "$ROOT_DIR/target/release/pool-server"
else
    echo "错误: 找不到pool-server可执行文件。请先运行'make release'来构建它"
    echo "Error: Cannot find pool-server executable. Please run 'make release' to build it first"
    exit 1
fi 