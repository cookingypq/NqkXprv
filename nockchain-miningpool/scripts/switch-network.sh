#!/bin/bash
# 一键切换主网/测试网
# 用法: ./switch-network.sh [mainnet|fakenet]
MODE="$1"
ENV_FILE="$(dirname "$0")/../.env"
if [[ "$MODE" == "mainnet" ]]; then
    echo "切换到主网配置..."
    sed -i '' 's/^FAKENET=.*/FAKENET=false/' "$ENV_FILE" 2>/dev/null || echo "FAKENET=false" >> "$ENV_FILE"
    sed -i '' 's/^NO_DEFAULT_PEERS=.*/NO_DEFAULT_PEERS=false/' "$ENV_FILE" 2>/dev/null || echo "NO_DEFAULT_PEERS=false" >> "$ENV_FILE"
    sed -i '' 's/^FAKENET_POW_LEN=.*/FAKENET_POW_LEN=/' "$ENV_FILE" 2>/dev/null
    sed -i '' 's/^FAKENET_LOG_DIFFICULTY=.*/FAKENET_LOG_DIFFICULTY=/' "$ENV_FILE" 2>/dev/null
    sed -i '' 's/^FAKENET_GENESIS_JAM_PATH=.*/FAKENET_GENESIS_JAM_PATH=/' "$ENV_FILE" 2>/dev/null
    # 如果没有PEER字段则添加注释和空PEER
    if ! grep -q "^PEER=" "$ENV_FILE"; then
        echo "# PEER=主网节点1:端口,主网节点2:端口  # 请填写主网节点地址，逗号分隔" >> "$ENV_FILE"
        echo "PEER=" >> "$ENV_FILE"
    fi
    echo "已切换到主网。请在.env中填写主网节点PEER=...，然后重启相关服务。"
else
    echo "切换到测试网（fakenet）配置..."
    sed -i '' 's/^FAKENET=.*/FAKENET=true/' "$ENV_FILE" 2>/dev/null || echo "FAKENET=true" >> "$ENV_FILE"
    sed -i '' 's/^NO_DEFAULT_PEERS=.*/NO_DEFAULT_PEERS=true/' "$ENV_FILE" 2>/dev/null || echo "NO_DEFAULT_PEERS=true" >> "$ENV_FILE"
    sed -i '' 's/^FAKENET_POW_LEN=.*/FAKENET_POW_LEN=3/' "$ENV_FILE" 2>/dev/null || echo "FAKENET_POW_LEN=3" >> "$ENV_FILE"
    sed -i '' 's/^FAKENET_LOG_DIFFICULTY=.*/FAKENET_LOG_DIFFICULTY=21/' "$ENV_FILE" 2>/dev/null || echo "FAKENET_LOG_DIFFICULTY=21" >> "$ENV_FILE"
    sed -i '' 's/^FAKENET_GENESIS_JAM_PATH=.*/FAKENET_GENESIS_JAM_PATH=/' "$ENV_FILE" 2>/dev/null
    # 清空PEER
    sed -i '' 's/^PEER=.*/PEER=/' "$ENV_FILE" 2>/dev/null
    echo "已切换到测试网。请重启相关服务。"
fi 