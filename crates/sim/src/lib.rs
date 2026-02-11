//! Deterministic simulation crate for Halo Wars network compatibility.
//!
//! This crate implements the core simulation logic that must match vanilla
//! Halo Wars exactly for network compatibility. The wire format for commands
//! must be byte-for-byte identical.

pub mod command;
pub mod commands;
pub mod dispatcher;
pub mod entity;
pub mod entity_id;
pub mod packet;
pub mod random;
pub mod serialize;
pub mod session;
pub mod sync;
pub mod time_sync;

pub use command::{Command, CommandType, EntityType};
pub use commands::{GameCommand, PowerCommand, WorkCommand};
pub use dispatcher::{CommandDispatcher, DispatchError, DispatchedCommand};
pub use entity::{Entity, EntityManager};
pub use entity_id::{EntityClass, EntityId};
pub use packet::{ChannelPacketHeader, PacketError};
pub use random::Random;
pub use serialize::{SerializeError, deserialize_command, serialize_command};
pub use session::{ClientState, PlayerInfo, Session, SessionState};
pub use sync::{SimpleChecksum, SyncChecksum};
pub use time_sync::{ClientTimeHistory, TimeSync, TimingRecord};
