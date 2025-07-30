todo: 

✅ Client/server model: pool-server acts as the gRPC server, and miner acts as the gRPC client.
✅ Binary products: pool-server and miner are two independent executable files.
✅ Separation of duties: the command center and the computing unit are completely separated.
Communication protocol compliance:
✅ gRPC protocol: implemented using tonic, fully compliant with proto definitions.
✅ Bidirectional streaming: The Subscribe method implements server task push and client heartbeat
✅ Task submission: The SubmitWork method handles the submission of computing results
User experience compliance:
✅ One-click startup: Provides Makefile and script support
✅ Command-line parameters: Supports pool-server and miner <IP> -t <number of threads> format
✅ Out-of-the-box: Pre-compiled binary files, no compilation required
Workflow compliance:
✅ Automatic task flow: Complete mining closed-loop process
✅ Real-time push: Immediately push new tasks when the blockchain status changes
✅ Efficient verification: Fast PoW verification and block submission

⭕️ Implement pool-server synchronization on the main network



Pool-Server synchronization of mainnet function call chains：

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
│                   │   └── Regular expression parsing of “Latest Block” number
│                   └── estimate_network_height() (fallback method)
│                       └── current_height + 10 (estimate)
│
├── Periodic blockchain status check task (every 10 seconds)
│   ├── get_current_block_id()
│   │   └── peek [%heavy ~] Get current block ID
│   ├── Compare block ID changes
│   ├── Update status monitor
│   │   ├── update_block_hash()
│   │   ├── update_block_height()
│   │   └── update_network_block_height ()
│   └── Generate new work task
│       ├── generate_work()
│       │   └── get_chain_state()
│       │       ├── peek [%heavy ~] Get latest block ID
│       │       ├── peek [%block <block-id> ~] Get block page
│       │       ├── parse block height, target difficulty, parent block hash
│       │       └── calculate Merkle root
│       └── broadcast_work() broadcast to all miners
│
└── Regular network height check (every 60 seconds)
    └── try_get_latest_network_height()
        └── network_manager.get_latest_network_height()