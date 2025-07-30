这份文档将包含两部分：
1.  **产品设计文档 (PDD):** 定义“我们要做什么”以及“它为客户带来什么价值”。
2.  **工程设计文档 (EDD):** 定义“我们将如何技术实现”。

---

### **最终版本 - 产品设计文档 (Product Design Document)**

**产品名称：** `Nockchain` 矿池挖矿套件

**核心目标：** 打造一个高效、易于部署的矿池系统，使大规模矿工集群（特指局域网环境）能够将区块链同步与PoW计算任务分离，从而极大地提升整体挖矿效率和资源利用率。

**1. 核心角色定义：指挥中心与计算兵团**
*   **描述：** 本套件包含两个核心角色，由两个独立的程序扮演：
    *   **指挥中心 (`pool-server`)：** 整个矿场的大脑。它是在集群中唯一需要运行完整`nockchain`节点、与主网同步、处理区块和交易的程序。它负责生成计算任务，并分发给所有矿工。
    *   **计算矿工 (`miner`)：** 纯粹的计算兵团。它不下载区块链，不验证交易，唯一的职责就是从指挥中心获取计算任务，执行高强度的PoW运算，并将有效结果提交回去。
*   **价值：** 明确的职责分离。让179台计算矿工机器摆脱了同步区块链的沉重负担（磁盘、内存、带宽消耗），使其CPU资源100%用于挖矿计算，实现专业化和效率最大化。

**2. 目标场景：为局域网而生**
*   **描述：** 本方案专为客户的“180台机器位于同一局域网”的场景深度优化。所有通信设计都基于内网环境的低延迟、高带宽和高稳定性特性。
*   **价值：** 通信效率极高，当指挥中心因为链上状态变化而生成新任务时，可以通过推送机制瞬时通知到所有179台矿机，将无效计算时间降至毫秒级。同时，配置部署极其简单，无需处理复杂的公网穿透或NAT问题。

**3. 用户体验：一键启动与开箱即用**
*   **描述：** 我们将提供两个预编译好的Linux可执行文件，客户无需任何编译操作。操作流程被设计得极其简单：
    *   **启动指挥中心 (1台机器):**
        ```bash
        ./pool-server
        ```
    *   **启动计算矿工 (179台机器):**
        ```bash
        ./miner <指挥中心IP> -t <线程数>
        # 示例: ./miner 192.168.1.100 -t 16
        ```
*   **价值：** 极大地降低了客户的部署和运维门槛。客户只需分发文件，用简单指令即可启动和管理整个矿场。清晰的命令行参数设计也提供了必要的灵活性（如调整线程数）。

**4. 核心工作流：自动化任务流转**
*   **描述：** 客户启动程序后，系统将按以下流程自动运行：
    1.  `miner` 启动，连接到指定的 `pool-server` IP。
    2.  `pool-server` 接受连接，并立即从其同步好的区块链数据中，生成一份计算任务（Work Order）发送给 `miner`。
    3.  `miner` 接收任务，调用硬件资源开始计算 `nonce`。
    4.  当 `miner` 找到有效 `nonce`，立刻将其提交给 `pool-server`。
    5.  `pool-server` 验证 `nonce`，如果有效，则将完整区块广播到 `nockchain` 主网，并立即生成一份**新**的任务，主动推送给**所有**已连接的 `miner`。
*   **价值：** 整个挖矿过程形成了一个高效的闭环，最大化了算力的有效利用率，并确保一旦有区块产出，所有算力能立刻转向下一个新区块的竞争。

**5. 价值主张：降本增效，专业分工**
*   **描述：** 本方案的核心价值是让客户的180台机器集群发挥出远超SOLO模式的挖矿效益。
*   **价值：**
    *   **降本：** 大幅降低对矿工机器的硬件要求（尤其是磁盘和内存）。
    *   **增效：** 将所有计算资源聚焦于核心的PoW计算，杜绝了因区块同步延迟或重复劳动造成的算力浪费。
    *   **易管理：** 通过指挥中心，可以集中监控整个矿场的运行状态，便于统一管理和问题排查。

---

### **最终版本 - 工程设计文档 (Engineering Design Document)**

**核心架构：** 客户端/服务器（C/S）模型。`pool-server` 作为有状态的gRPC服务端，`miner` 作为无状态的gRPC客户端。

**1. 系统组件与二进制产物**
*   **`pool-server` (新 Crate):**
    *   **来源：** 在 `nockchain-miningpool/crates/` 目录下创建 `nockchain-pool-server`。
    *   **功能：** 内嵌一个 `nockchain` 核心库实例作为全节点；同时，启动一个 `gRPC` 服务端，用于管理矿工连接和任务分发。
    *   **二进制文件名：** 通过 `Cargo.toml` 配置，编译产物为 `pool-server`。
*   **`miner` (改造 Crate):**
    *   **来源：** 改造现有的 `crates/nockchain`。
    *   **功能：** 程序逻辑将分为两种模式。通过启动时是否提供“指挥中心IP”来切换。
        *   **矿池模式（提供IP）：** 禁用所有P2P和同步逻辑，作为轻量级gRPC客户端运行。
        *   **SOLO模式（不提供IP）：** 保持现有逻辑不变，作为独立的全节点矿工运行（用于兼容和测试）。
    *   **二进制文件名：** 通过 `Cargo.toml` 配置，编译产物从 `nockchain` 重命名为 `miner`。

**2. 矿池通信协议 (Pool Protocol via gRPC)**
*   **技术栈：** `tonic` (Rust gRPC 实现)。
*   **Proto定义 (`pool.proto`):**
    ```protobuf
    syntax = "proto3";
    package pool;

    // 服务定义
    service MiningPool {
      // 矿工上线后，建立一个双向流，用于服务器推送任务和客户端上报心跳/状态
      rpc Subscribe(stream MinerStatus) returns (stream WorkOrder);
      // 矿工提交计算结果
      rpc SubmitWork(WorkResult) returns (SubmitAck);
    }

    // 工作任务：包含PoW所需的一切
    message WorkOrder {
      string work_id = 1;         // 任务ID
      bytes parent_hash = 2;      // 父区块哈希
      bytes merkle_root = 3;      // 交易默克尔根
      uint64 timestamp = 4;       // 时间戳
      bytes difficulty_target = 5; // 难度目标
    }

    // 计算结果
    message WorkResult {
      string work_id = 1;        // 对应的任务ID
      bytes nonce = 2;           // 找到的Nonce
      string miner_id = 3;       // 矿工ID（可选，用于日志）
    }

    // 提交确认
    message SubmitAck {
      bool success = 1; // 是否接受
    }

    // 矿工状态/心跳
    message MinerStatus {
      string miner_id = 1;      // 矿工ID
      uint32 threads = 2;       // 使用的线程数
      // ... 未来可扩展算力等信息
    }
    ```
*   **设计 rationale：** 使用服务器流 (`Subscribe`) 替代简单的 `GetWork` 调用，可以由服务器在链状态更新时**主动**、**立即**将新任务推送给所有矿工，这是局域网环境下的最佳实践。

**3. `miner` 客户端改造计划**
*   **命令行解析：** 使用 `clap` 库，在 `crates/nockchain/src/main.rs` 中定义命令行接口：一个必需的位置参数 `server_ip`，一个可选的 `-t, --threads` 参数。
*   **逻辑分叉：**
    *   在 `main` 函数的入口处，检查 `server_ip` 是否被提供。
    *   **if Some(ip):**
        1.  完全跳过 `nockchain-libp2p-io` 初始化、`chain` 数据库加载等所有与SOLO模式相关的设置。
        2.  实例化 `tonic` gRPC 客户端，连接到 `ip`。
        3.  调用 `client.subscribe()` 与服务器建立持久连接。
        4.  开启一个新`tokio`任务，在此任务中进入一个 `while let Some(work_order) = stream.message().await?` 的循环，不断接收服务器推送的任务。
        5.  在循环中，将收到的 `work_order` 交给 `crates/kernels/src/miner.rs` 中的现有PoW计算函数进行运算。
        6.  如果找到解，则通过 `client.submit_work()` 提交。
    *   **if None:** 执行原始的 `main` 函数逻辑，启动SOLO模式。

**4. `pool-server` 服务端实现计划**
*   **项目依赖：** 添加 `tokio`, `tonic`, `nockchain` (作为库)。
*   **核心逻辑 (`nockchain-pool-server/src/main.rs`):**
    1.  **启动：** 异步 `main` 函数，初始化 `tracing` 日志系统，启动内嵌的 `nockchain` 核心节点实例，并启动gRPC服务器。
    2.  **连接管理：** 维护一个线程安全的 `HashMap`，用于存储所有已连接矿工的 `sender` 通道，键为矿工ID或连接ID。
    3.  **任务生成与广播：** 监听内嵌节点的链头更新事件。一旦事件触发，立即从内嵌节点的交易池中打包交易，生成新的 `WorkOrder`。然后遍历 `HashMap`，将这个新任务通过每个矿工的 `sender` 通道广播出去。
    4.  **`Subscribe` 实现：** 当一个新矿工连接时，将其 `sender` 存入 `HashMap`，并立即发送当前的最新 `WorkOrder` 给它。
    5.  **`SubmitWork` 实现：**
        *   这是一个高频调用的无状态函数，必须极快。
        *   根据 `WorkResult` 中的 `work_id` 找到当时下发任务的上下文。
        *   快速执行PoW验证。
        *   若验证成功，则构建完整区块，通过内嵌节点实例将其广播到主网。

**5. 构建与交付策略**
*   **`Makefile` / 构建脚本：** 提供一个顶层 `Makefile`，包含一个 `release` 目标：
    ```makefile
    .PHONY: release
    release:
        @echo "Building release binaries for pool-server and miner..."
        @cargo build --release --workspace
        @echo "Binaries are available in target/release/"
    ```
*   **静态链接：** 在 `Cargo.toml` 中配置，尽可能尝试静态链接，以减少对目标Linux系统库的依赖，提升可移植性。
*   **交付物：** `target/release/pool-server` 和 `target/release/miner` 两个文件。