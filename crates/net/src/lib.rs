//! Network transport crate for Halo Wars multiplayer.
//!
//! Provides UDP-based networking for deterministic lockstep multiplayer.

pub mod client;
pub mod connection;
pub mod host;
pub mod packet;
pub mod transport;

pub use client::NetClient;
pub use connection::{Connection, ConnectionState};
pub use host::NetHost;
pub use packet::{NetPacket, PacketType};
pub use transport::Transport;
