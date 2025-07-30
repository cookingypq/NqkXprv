This document contains two parts:
1. **Product Design Document (PDD):** Defines "what we are building" and "what value it brings to customers".
2. **Engineering Design Document (EDD):** Defines "how we will implement it technically".

---

### **Final Version - Product Design Document (Product Design Document)**

**Product Name:** `Nockchain` Mining Pool Suite

**Core Objective:** Build an efficient, easy-to-deploy mining pool system that enables large-scale miner clusters (specifically in LAN environments) to separate blockchain synchronization from PoW computation tasks, thereby greatly improving overall mining efficiency and resource utilization.

**1. Core Role Definition: Command Center and Computing Army**
*   **Description:** This suite contains two core roles, played by two independent programs:
    *   **Command Center (`pool-server`):** The brain of the entire mining farm. It is the only program in the cluster that needs to run a complete `nockchain` node, synchronize with the mainnet, and process blocks and transactions. It is responsible for generating computation tasks and distributing them to all miners.
    *   **Computing Miners (`miner`):** Pure computing army. They do not download the blockchain, do not validate transactions, and their only responsibility is to obtain computation tasks from the command center, perform high-intensity PoW calculations, and submit valid results back.
*   **Value:** Clear separation of responsibilities. This frees 179 computing miner machines from the heavy burden of synchronizing the blockchain (disk, memory, bandwidth consumption), allowing their CPU resources to be 100% dedicated to mining calculations, achieving specialization and efficiency maximization.

**2. Target Scenario: Born for LAN**
*   **Description:** This solution is deeply optimized for the customer's scenario of "180 machines located in the same LAN". All communication design is based on the low-latency, high-bandwidth, and high-stability characteristics of the internal network environment.
*   **Value:** Extremely high communication efficiency. When the command center generates new tasks due to on-chain state changes, it can instantly notify all 179 mining machines through push mechanisms, reducing invalid computation time to milliseconds. At the same time, configuration and deployment are extremely simple, without the need to handle complex public network penetration or NAT issues.

**3. User Experience: One-Click Startup and Out-of-the-Box Usage**
*   **Description:** We will provide two pre-compiled Linux executable files, and customers need no compilation operations. The operation process is designed to be extremely simple:
    *   **Start Command Center (1 machine):**
        ```bash
        ./pool-server
        ```
    *   **Start Computing Miners (179 machines):**
        ```bash
        ./miner <command-center-IP> -t <thread-count>
        # Example: ./miner 192.168.1.100 -t 16
        ```
*   **Value:** Greatly reduces the deployment and operation threshold for customers. Customers only need to distribute files and use simple commands to start and manage the entire mining farm. Clear command-line parameter design also provides necessary flexibility (such as adjusting thread count).

**4. Core Workflow: Automated Task Circulation**
*   **Description:** After customers start the programs, the system will automatically run according to the following process:
    1.  `miner` starts and connects to the specified `pool-server` IP.
    2.  `pool-server` accepts the connection and immediately generates a computation task (Work Order) from its synchronized blockchain data and sends it to the `miner`.
    3.  `miner` receives the task and calls hardware resources to start computing `nonce`.
    4.  When `miner` finds a valid `nonce`, it immediately submits it to `pool-server`.
    5.  `pool-server` validates the `nonce`, and if valid, broadcasts the complete block to the `nockchain` mainnet and immediately generates a **new** task, actively pushing it to **all** connected `miner`s.
*   **Value:** The entire mining process forms an efficient closed loop, maximizing the effective utilization of computing power and ensuring that once a block is produced, all computing power can immediately turn to compete for the next new block.

**5. Value Proposition: Cost Reduction and Efficiency Improvement, Professional Division of Labor**
*   **Description:** The core value of this solution is to enable the customer's 180-machine cluster to achieve mining benefits far exceeding SOLO mode.
*   **Value:**
    *   **Cost Reduction:** Significantly reduces hardware requirements for miner machines (especially disk and memory).
    *   **Efficiency Improvement:** Focuses all computing resources on core PoW calculations, eliminating computing power waste caused by block synchronization delays or redundant work.
    *   **Easy Management:** Through the command center, centralized monitoring of the entire mining farm's operation status is possible, facilitating unified management and problem troubleshooting.

---

### **Final Version - Engineering Design Document (Engineering Design Document)**

**Core Architecture:** Client/Server (C/S) model. `pool-server` serves as a stateful gRPC server, `miner` serves as a stateless gRPC client.

**1. System Components and Binary Artifacts**
*   **`pool-server` (New Crate):**
    *   **Source:** Create `nockchain-pool-server` in the `nockchain-miningpool/crates/` directory.
    *   **Functionality:** Embeds a `nockchain` core library instance as a full node; simultaneously starts a `gRPC` server for managing miner connections and task distribution.
    *   **Binary Filename:** Configure through `Cargo.toml`, with compilation output as `pool-server`.
*   **`miner` (Modified Crate):**
    *   **Source:** Modify the existing `crates/nockchain`.
    *   **Functionality:** Program logic will be divided into two modes. Switch by whether "command center IP" is provided at startup.
        *   **Pool Mode (IP provided):** Disable all P2P and synchronization logic, run as a lightweight gRPC client.
        *   **SOLO Mode (no IP provided):** Keep existing logic unchanged, run as an independent full-node miner (for compatibility and testing).
    *   **Binary Filename:** Configure through `Cargo.toml`, with compilation output renamed from `nockchain` to `miner`.

**2. Mining Pool Communication Protocol (Pool Protocol via gRPC)**
*   **Technology Stack:** `tonic` (Rust gRPC implementation).
*   **Proto Definition (`pool.proto`):**
    ```protobuf
    syntax = "proto3";
    package pool;

    // Service definition
    service MiningPool {
      // After miners come online, establish a bidirectional stream for server to push tasks and client to report heartbeats/status
      rpc Subscribe(stream MinerStatus) returns (stream WorkOrder);
      // Miner submits computation results
      rpc SubmitWork(WorkResult) returns (SubmitAck);
    }

    // Work task: contains everything needed for PoW
    message WorkOrder {
      string work_id = 1;         // Task ID
      bytes parent_hash = 2;      // Parent block hash
      bytes merkle_root = 3;      // Transaction merkle root
      uint64 timestamp = 4;       // Timestamp
      bytes difficulty_target = 5; // Difficulty target
    }

    // Computation result
    message WorkResult {
      string work_id = 1;        // Corresponding task ID
      bytes nonce = 2;           // Found nonce
      string miner_id = 3;       // Miner ID (optional, for logging)
    }

    // Submission acknowledgement
    message SubmitAck {
      bool success = 1; // Whether accepted
    }

    // Miner status/heartbeat
    message MinerStatus {
      string miner_id = 1;      // Miner ID
      uint32 threads = 2;       // Number of threads used
      // ... Future extensibility for hashrate information
    }
    ```
*   **Design Rationale:** Using server stream (`Subscribe`) instead of simple `GetWork` calls allows the server to **actively** and **immediately** push new tasks to all miners when chain state updates, which is the best practice in LAN environments.

**3. `miner` Client Modification Plan**
*   **Command Line Parsing:** Use the `clap` library to define command-line interface in `crates/nockchain/src/main.rs`: one required positional argument `server_ip`, one optional `-t, --threads` argument.
*   **Logic Branching:**
    *   At the entry point of the `main` function, check whether `server_ip` is provided.
    *   **if Some(ip):**
        1.  Completely skip `nockchain-libp2p-io` initialization, `chain` database loading, and all other SOLO mode-related settings.
        2.  Instantiate `tonic` gRPC client, connect to `ip`.
        3.  Call `client.subscribe()` to establish persistent connection with server.
        4.  Start a new `tokio` task, enter a `while let Some(work_order) = stream.message().await?` loop in this task, continuously receiving tasks pushed by the server.
        5.  In the loop, hand the received `work_order` to the existing PoW calculation function in `crates/kernels/src/miner.rs` for computation.
        6.  If a solution is found, submit it via `client.submit_work()`.
    *   **if None:** Execute the original `main` function logic, start SOLO mode.

**4. `pool-server` Server Implementation Plan**
*   **Project Dependencies:** Add `tokio`, `tonic`, `nockchain` (as library).
*   **Core Logic (`nockchain-pool-server/src/main.rs`):**
    1.  **Startup:** Async `main` function, initialize `tracing` logging system, start embedded `nockchain` core node instance, and start gRPC server.
    2.  **Connection Management:** Maintain a thread-safe `HashMap` to store `sender` channels for all connected miners, with miner ID or connection ID as the key.
    3.  **Task Generation and Broadcasting:** Listen for chain head update events from the embedded node. Once an event is triggered, immediately package transactions from the embedded node's transaction pool, generate a new `WorkOrder`, then iterate through the `HashMap` and broadcast this new task to each miner via their `sender` channel.
    4.  **`Subscribe` Implementation:** When a new miner connects, store its `sender` in the `HashMap` and immediately send the current latest `WorkOrder` to it.
    5.  **`SubmitWork` Implementation:**
        *   This is a high-frequency stateless function that must be extremely fast.
        *   Find the context of the task issued at that time based on the `work_id` in `WorkResult`.
        *   Quickly execute PoW validation.
        *   If validation succeeds, build the complete block and broadcast it to the mainnet via the embedded node instance.

**5. Build and Delivery Strategy**
*   **`Makefile` / Build Scripts:** Provide a top-level `Makefile` with a `release` target:
    ```makefile
    .PHONY: release
    release:
        @echo "Building release binaries for pool-server and miner..."
        @cargo build --release --workspace
        @echo "Binaries are available in target/release/"
    ```
*   **Static Linking:** Configure in `Cargo.toml` to attempt static linking as much as possible to reduce dependency on target Linux system libraries and improve portability.
*   **Deliverables:** Two files: `target/release/pool-server` and `target/release/miner`.
