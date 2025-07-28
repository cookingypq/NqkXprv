#!/bin/bash

# 一键打包脚本 - 创建 nock-release-0.03.tar.gz
set -e

PACKAGE_NAME="nock-release-0.03"
TEMP_DIR="temp_package"
RELEASE_DIR="target/release"

# 清理临时目录
if [ -d "$TEMP_DIR" ]; then
    rm -rf "$TEMP_DIR"
fi

# 创建临时目录结构
mkdir -p "$TEMP_DIR/bin"
mkdir -p "$TEMP_DIR/jams"
mkdir -p "$TEMP_DIR/data"

# 复制二进制文件
cp "$RELEASE_DIR/pool-server" "$TEMP_DIR/bin/"
cp "$RELEASE_DIR/miner" "$TEMP_DIR/bin/"
chmod +x "$TEMP_DIR/bin/"*

# 复制环境配置文件
cp .env_example "$TEMP_DIR/.env_example"

# 复制所有 jam 文件
cp hoon/jams/*.jam "$TEMP_DIR/jams/" 2>/dev/null || true
cp hoon/test-jams/*.jam "$TEMP_DIR/jams/" 2>/dev/null || true
cp crates/nockchain/jams/*.jam "$TEMP_DIR/jams/" 2>/dev/null || true
cp crates/nockapp/test-jams/*.jam "$TEMP_DIR/jams/" 2>/dev/null || true

# 复制区块链快照数据（如果存在）
if [ -d .data.nockchain/checkpoints ]; then
    mkdir -p "$TEMP_DIR/data/checkpoints"
    cp .data.nockchain/checkpoints/*.chkjam "$TEMP_DIR/data/checkpoints/" 2>/dev/null || true
fi

# 创建简单 README
cat > "$TEMP_DIR/README.md" << 'EOF'
# Nockchain Release 0.03 (精简包)

## 包含内容
- bin/pool-server
- bin/miner
- jams/*.jam
- .env_example
- data/checkpoints/*.chkjam（如存在，为区块链快照）

## 使用方法
1. 解压包：
   ```bash
   tar -xzf nock-release-0.03.tar.gz
   cd nock-release-0.03
   ```
2. 复制并编辑环境变量：
   ```bash
   cp .env_example .env
   # 编辑 .env 文件
   ```
3. （可选）恢复区块链快照：
   ```bash
   # 如果需要恢复历史区块链状态
   mkdir -p .data.nockchain/checkpoints
   cp data/checkpoints/*.chkjam .data.nockchain/checkpoints/
   ```
4. 运行：
   ```bash
   ./bin/pool-server
   ./bin/miner
   ```
EOF

# 打包
 tar -czf "${PACKAGE_NAME}.tar.gz" -C "$TEMP_DIR" .
rm -rf "$TEMP_DIR"

echo "打包完成: ${PACKAGE_NAME}.tar.gz"
ls -lh "${PACKAGE_NAME}.tar.gz" 