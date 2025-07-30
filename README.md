# NqkXprv Project

## Project Status

### ✅ Completed Features

#### Client/Server Model
- **Pool-Server**: Acts as gRPC server
- **Miner**: Acts as gRPC client
- **Binary Products**: pool-server and miner are two independent executable files
- **Separation of Duties**: Command center and computing unit are completely separated

#### Communication Protocol Compliance
- **gRPC Protocol**: Implemented using tonic, fully compliant with proto definitions
- **Bidirectional Streaming**: Subscribe method implements server task push and client heartbeat
- **Task Submission**: SubmitWork method handles the submission of computing results

#### User Experience Compliance
- **One-click Startup**: Provides Makefile and script support
- **Command-line Parameters**: Supports `pool-server` and `miner <IP> -t <threads>` format
- **Out-of-the-box**: Pre-compiled binary files, no compilation required

#### Workflow Compliance
- **Automatic Task Flow**: Complete mining closed-loop process
- **Real-time Push**: Immediately push new tasks when blockchain status changes
- **Efficient Verification**: Fast PoW verification and block submission

### ⭕️ Pending Features

- **Mainnet Synchronization**: Implement pool-server synchronization on the main network

## Mainnet Synchronization Function Call Chain

### Initialization Flow

```
main() 
├── MiningPoolService::new(nockapp)
│   └── start_blockchain_event_listener()
│       └── initial_sync_with_mainnet()
│           ├── verify_mainnet_genesis()
│           │   ├── peek [%genesis-seal-set ~] Check genesis block settings
│           │   └── peek [%genesis-seal ~] Verify mainnet genesis information
│           │       └── set_realnet_genesis_seal() (if necessary)
│           │           └── poke [%command %set-genesis-seal height seal]
│           │
│           ├── get_current_block_height()
│           │   ├── peek [%heavy ~] Get the latest block ID
│           │   └── peek [%block <block-id> ~] Get block information
│           │
│           └── try_get_latest_network_height()
│               └── network_manager.get_latest_network_height()
│                   ├── fetch_height_from_nockblocks() (main method)
│                   │   ├── HTTP GET https://nockblocks.com/blocks
│                   │   └── Regular expression parsing of "Latest Block" number
│                   └── estimate_network_height() (fallback method)
│                       └── current_height + 10 (estimate)
```

### Periodic Tasks

#### Blockchain Status Check (every 10 seconds)
```
├── get_current_block_id()
│   └── peek [%heavy ~] Get current block ID
├── Compare block ID changes
├── Update status monitor
│   ├── update_block_hash()
│   ├── update_block_height()
│   └── update_network_block_height()
└── Generate new work task
    ├── generate_work()
    │   └── get_chain_state()
    │       ├── peek [%heavy ~] Get latest block ID
    │       ├── peek [%block <block-id> ~] Get block page
    │       ├── Parse block height, target difficulty, parent block hash
    │       └── Calculate Merkle root
    └── broadcast_work() broadcast to all miners
```

#### Network Height Check (every 60 seconds)
```
└── try_get_latest_network_height()
    └── network_manager.get_latest_network_height()
```

## Project Structure

```
NqkXprv/
├── nockchain-miningpool/          # Main project directory
│   ├── crates/                    # Rust modules
│   │   ├── nockchain/            # Blockchain core
│   │   ├── nockchain-pool-server/ # Pool server
│   │   ├── nockchain-wallet/     # Wallet functionality
│   │   └── nockvm/               # Virtual machine
│   ├── hoon/                      # Hoon language code
│   ├── scripts/                   # Run scripts
│   └── logs/                      # Log files
├── PDD_EDD_ch.md                 # Chinese design documents
├── PDD&EDD_en.md                 # English design documents
└── README.md                      # Project description
```

## Quick Start

### Mac OS Setup 
```bash
cd nockchain-miningpool && mkdir -p assets
```

```bash
make install-hoonc
```

```bash
make build-hoon-all
```

```bash
cargo install protobuf-codegen
```

```bash
make pool-full
```

### Start Pool-Server
```bash
./target/release/pool-server
```

### Start Miner
```bash
./target/release/miner <pool-server-ip>:<pool-server-port-number> -t <threads>
```

### Using Makefile
```bash
make start-pool
make start-miner POOL_IP=<ip> THREADS=<num>
```

## Development Status

- ✅ Basic architecture completed
- ✅ Communication protocol implemented
- ✅ User interface optimized
- ⭕️ Mainnet synchronization pending
- ⭕️ Performance optimization in progress