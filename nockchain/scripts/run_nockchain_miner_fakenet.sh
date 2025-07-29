#!/bin/bash
source .env
export RUST_LOG
export MINIMAL_LOG_FORMAT
export MINING_PUBKEY
rm -rf nockchain.sock
nockchain --mine --fakenet --npc-socket nockchain.sock --mining-pubkey ${MINING_PUBKEY} --peer /ip4/127.0.0.1/udp/3006/quic-v1
