# Nockchain矿池状态监控系统

本文档提供了Nockchain矿池状态监控系统的使用指南，包括如何编译、配置和运行矿池服务器和矿工客户端。

## 功能概述

矿池状态监控系统提供以下功能：

1. **定期状态日志**：矿池服务器会定期将状态摘要记录到日志中
2. **详细的日志记录**：为关键操作添加了更详细的日志
3. **区块链事件监听**：监听区块高度变化并记录
4. **统计信息收集**：记录提交区块数、总算力等
5. **HTTP状态API**：提供简单的HTTP API查询矿池状态

## 编译指南

### 编译矿池服务器和矿工客户端

使用以下命令同时编译矿池服务器和矿工客户端：

```bash
cd nockchain-miningpool
make pool-full
```

或者分别编译：

```bash
# 编译矿池服务器
make pool-server

# 编译矿工客户端（矿池模式）
make miner
```

### 安装到系统路径

```bash
# 安装矿池完整套件
make install-pool-full

# 或者分别安装
make install-pool-server
make install-nockchain
```

## 配置指南

### 矿池服务器配置

创建`.env`文件（可以从`.env_example`复制），设置以下参数：

```
# 矿工收款公钥
MINING_PUBKEY="你的挖矿公钥..."

# 日志级别设置：trace, debug, info, warn, error
RUST_LOG=info

# 矿池服务器监听地址
POOL_SERVER_ADDRESS=0.0.0.0:7777

# HTTP API服务器监听地址
HTTP_API_ADDRESS=0.0.0.0:8080

# 状态日志输出间隔（秒）
STATUS_LOG_INTERVAL=300
```

## 运行指南

### 启动矿池服务器

```bash
./scripts/start-pool-server.sh
```

### 启动矿工客户端

```bash
# 首次运行或需要重新编译时添加--rebuild参数
./scripts/start-miner.sh 矿池服务器IP:端口 --threads 线程数 --rebuild

# 示例：连接到本地矿池服务器，使用4个线程
./scripts/start-miner.sh 127.0.0.1:7777 --threads 4 --rebuild
```

## 监控矿池状态

### 使用监控脚本

我们提供了一个简单的命令行监控脚本：

```bash
./scripts/monitor-pool.sh [主机地址:端口]

# 默认连接到localhost:8080
./scripts/monitor-pool.sh

# 连接到特定服务器
./scripts/monitor-pool.sh 192.168.1.100:8080
```

### 使用HTTP API

矿池服务器提供以下HTTP API接口：

1. **完整状态** - http://服务器IP:8080/api/status
   - 提供全面的矿池状态信息

2. **简化状态** - http://服务器IP:8080/api/basic
   - 提供简化的状态信息（矿工数、哈希率、区块数等）

3. **健康检查** - http://服务器IP:8080/health
   - 用于监控服务器是否在线

示例（使用curl）：

```bash
# 获取基本状态
curl http://localhost:8080/api/basic | jq

# 获取完整状态
curl http://localhost:8080/api/status | jq
```

## 故障排除

### 常见问题

1. **矿池模式未启用**
   - 错误信息：`矿池模式未启用，请使用 --features pool_client 重新编译`
   - 解决方案：使用 `--rebuild` 参数启动矿工客户端，或手动运行 `make miner`

2. **无法连接到矿池服务器**
   - 检查矿池服务器是否正在运行
   - 确认IP地址和端口正确
   - 确认网络连接和防火墙设置

3. **HTTP API无法访问**
   - 确认HTTP_API_ADDRESS配置正确
   - 检查防火墙设置是否允许访问指定端口

## 日志解读

矿池服务器会定期输出类似以下的状态摘要：

```
[2023-07-23T13:00:00Z INFO] ============ 矿池状态摘要 ============
[2023-07-23T13:00:00Z INFO] 运行时间: 3600 秒
[2023-07-23T13:00:00Z INFO] 连接矿工: 10 (总线程数: 80)
[2023-07-23T13:00:00Z INFO] 当前难度: 000000fffff...
[2023-07-23T13:00:00Z INFO] 哈希率: 160000000 H/s
[2023-07-23T13:00:00Z INFO] 区块高度: 1234
[2023-07-23T13:00:00Z INFO] 最后区块时间: 2023-07-23T12:58:30Z
[2023-07-23T13:00:00Z INFO] 找到区块: 5 (已接受: 5)
[2023-07-23T13:00:00Z INFO] 当前工作ID: abcd1234
[2023-07-23T13:00:00Z INFO] 工作更新次数: 15
[2023-07-23T13:00:00Z INFO] ======================================
```

通过以上信息，您可以了解矿池的运行状态、连接的矿工数量、总哈希率、区块高度等关键指标。