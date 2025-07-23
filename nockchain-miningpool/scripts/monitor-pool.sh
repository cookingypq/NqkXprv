#!/bin/bash
#
# 监控矿池状态脚本
# Pool Status Monitor Script
#
# 用法：./monitor-pool.sh [主机地址:端口]
# Usage: ./monitor-pool.sh [host:port]
#
# 默认主机地址为localhost:8080
# Default host is localhost:8080

# 设置默认主机地址
HOST=${1:-"localhost:8080"}
ENDPOINT="http://${HOST}/api/basic"

# 设置颜色
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 检查依赖
if ! command -v curl &> /dev/null; then
    echo -e "${RED}错误: 需要curl但未安装.${NC}"
    exit 1
fi

if ! command -v jq &> /dev/null; then
    echo -e "${RED}错误: 需要jq但未安装.${NC}"
    echo -e "${YELLOW}请使用以下命令安装: sudo apt install jq${NC}"
    exit 1
fi

echo -e "${BLUE}====================================${NC}"
echo -e "${BLUE}       矿池状态监控器 v1.0        ${NC}"
echo -e "${BLUE}====================================${NC}"
echo -e "${YELLOW}正在连接到 ${HOST}...${NC}"
echo ""

# 无限循环监控
while true; do
    # 清除屏幕
    clear
    
    # 获取当前时间
    CURRENT_TIME=$(date "+%Y-%m-%d %H:%M:%S")
    echo -e "${BLUE}===== 矿池状态 - ${CURRENT_TIME} =====${NC}"
    
    # 使用curl获取API数据
    RESPONSE=$(curl -s ${ENDPOINT})
    
    # 检查curl是否成功
    if [ $? -ne 0 ]; then
        echo -e "${RED}错误: 无法连接到矿池API!${NC}"
        echo -e "${YELLOW}请检查矿池服务器是否运行且API已启用.${NC}"
        sleep 5
        continue
    fi
    
    # 提取数据
    SUCCESS=$(echo $RESPONSE | jq -r '.success')
    
    if [ "$SUCCESS" != "true" ]; then
        echo -e "${RED}API返回错误!${NC}"
        echo -e "${YELLOW}响应: ${RESPONSE}${NC}"
        sleep 5
        continue
    fi
    
    # 解析数据
    MINERS=$(echo $RESPONSE | jq -r '.data.miners')
    HASHRATE=$(echo $RESPONSE | jq -r '.data.hashrate')
    BLOCKS=$(echo $RESPONSE | jq -r '.data.blocks')
    CURRENT_HEIGHT=$(echo $RESPONSE | jq -r '.data.current_height')
    
    # 格式化哈希率
    if [ $HASHRATE -ge 1000000000 ]; then
        FORMATTED_HASHRATE=$(echo "scale=2; $HASHRATE/1000000000" | bc)" GH/s"
    elif [ $HASHRATE -ge 1000000 ]; then
        FORMATTED_HASHRATE=$(echo "scale=2; $HASHRATE/1000000" | bc)" MH/s"
    elif [ $HASHRATE -ge 1000 ]; then
        FORMATTED_HASHRATE=$(echo "scale=2; $HASHRATE/1000" | bc)" KH/s"
    else
        FORMATTED_HASHRATE="$HASHRATE H/s"
    fi
    
    # 显示信息
    echo -e "${GREEN}连接矿工:${NC} $MINERS"
    echo -e "${GREEN}总哈希率:${NC} $FORMATTED_HASHRATE"
    echo -e "${GREEN}找到区块:${NC} $BLOCKS"
    echo -e "${GREEN}当前高度:${NC} $CURRENT_HEIGHT"
    echo -e "${BLUE}====================================${NC}"
    echo ""
    echo -e "${YELLOW}按 Ctrl+C 退出${NC}"
    
    # 等待10秒
    sleep 10
done