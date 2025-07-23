# Nockchain 矿池系统

## 简介

Nockchain 矿池系统是一个为大规模矿工集群设计的高效挖矿解决方案。它将区块链同步与PoW计算任务分离，使矿工机器能够专注于计算，从而大幅提升整体挖矿效率。

本系统包含两个核心组件：
- **矿池服务器 (`pool-server`)**：作为指挥中心，负责同步区块链、管理矿工和分发任务。一个矿场只需部署一个实例。
- **矿工客户端 (`miner`)**：作为计算节点，负责执行PoW计算任务。可在任意多的机器上部署。

## 快速开始

### 构建

```bash
# 构建所有组件
make release

# 或者单独构建各组件
make pool-server
make miner
make wallet
```

构建完成后，可执行文件将位于 `target/release/` 目录下。

### 部署流程

1. **准备工作**
   - 确保已构建 `pool-server`、`miner` 和 `nockchain-wallet` 三个可执行文件
   - 选择一台机器作为矿池服务器

2. **配置矿池服务器**
   - 生成挖矿公钥
     ```bash
     ./nockchain-wallet keygen
     ```
   - 创建 `.env` 文件，内容如下：
     ```
     MINING_PUBKEY=<你生成的公钥>
     RUST_LOG=info
     POOL_SERVER_ADDRESS=0.0.0.0:7777
     ```

3. **启动矿池服务器**
   ```bash
   ./scripts/start-pool-server.sh
   ```
   或者直接运行可执行文件：
   ```bash
   ./target/release/pool-server
   ```

4. **启动矿工客户端**
   在每台矿机上，运行：
   ```bash
   ./scripts/start-miner.sh <矿池服务器IP> --threads <线程数>
   ```
   或者直接运行可执行文件：
   ```bash
   ./target/release/miner <矿池服务器IP> --threads <线程数>
   ```
   
   例如：
   ```bash
   ./scripts/start-miner.sh 192.168.1.100 --threads 16
   ```

### 验证系统运行状态

- **矿池服务器日志**：显示已连接矿工数量和工作提交情况
- **矿工客户端日志**：显示连接状态、哈希率和提交工作的情况

## 脚本使用说明

### start-pool-server.sh

```bash
./scripts/start-pool-server.sh [选项]
```

选项：
- `-p, --pubkey KEY`：设置挖矿公钥
- `-a, --address ADDR`：设置服务器监听地址 (默认: 0.0.0.0:7777)
- `-l, --log LEVEL`：设置日志级别 (trace, debug, info, warn, error)
- `-h, --help`：显示帮助信息

### start-miner.sh

```bash
./scripts/start-miner.sh <矿池服务器地址> [选项]
```

选项：
- `-t, --threads N`：使用的线程数 (默认: CPU核心数减1)
- `-l, --log LEVEL`：设置日志级别 (trace, debug, info, warn, error)
- `-h, --help`：显示帮助信息

## 常见问题

### 矿工无法连接到矿池服务器

- 确认矿池服务器已正确启动
- 检查矿池服务器的IP地址是否正确
- 确认防火墙设置允许7777端口的TCP连接

### 矿工连接成功但未收到工作任务

- 检查矿池服务器日志，确认其已成功同步区块链
- 查看日志中是否有任何错误信息

### 查看钱包余额

使用`nockchain-wallet`工具查询余额：

```bash
./target/release/nockchain-wallet --nockchain-socket ./nockchain.sock list-notes-by-pubkey -p <您的公钥>
```

### 如何调整矿工性能

- 增减线程数可以直接影响挖矿性能，建议根据CPU核心数设置
- 对于有大量物理核心的服务器，可以考虑每个物理核心运行一个线程

## 系统架构

- **矿池服务器**: 嵌入了完整的nockchain节点，负责区块链同步、生成挖矿任务和验证提交的结果
- **矿工客户端**: 轻量级程序，仅包含PoW计算逻辑，不需要下载或验证区块链
- **通信协议**: 基于gRPC，提供高效的双向通信机制

这种架构设计使大规模矿场能够集中管理资源，提高算力利用率，同时降低每台矿机的硬件需求。
