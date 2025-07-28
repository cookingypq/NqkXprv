use serde_bytes::ByteBuf;
use equix;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum NockchainRequest {
    /// Request a block or TX from another node, carry PoW
    Request {
        pow: equix::SolutionByteArray,
        nonce: u64,
        message: ByteBuf,
    },
    /// Gossip a block or TX to another node
    Gossip { message: ByteBuf },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum NockchainResponse {
    /// The requested block or raw-tx
    Result { message: ByteBuf },
    /// If the request was a gossip, no actual response is needed
    Ack { acked: bool },
} 