//! Connection state management.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

/// A network connection to a remote peer.
#[derive(Debug)]
pub struct Connection {
    /// Remote address.
    pub addr: SocketAddr,
    /// Connection state.
    pub state: ConnectionState,
    /// Client ID assigned by host.
    pub client_id: u64,
    /// Last sequence number sent.
    pub local_sequence: u32,
    /// Last sequence number received.
    pub remote_sequence: u32,
    /// Ack bitfield for received packets.
    pub ack_bits: u32,
    /// Round-trip time estimate (ms).
    pub rtt_ms: u32,
    /// Last time we received a packet.
    pub last_recv_time: Instant,
    /// Last time we sent a packet.
    pub last_send_time: Instant,
    /// Connection timeout duration.
    pub timeout: Duration,
}

impl Connection {
    /// Create a new connection.
    #[must_use]
    pub fn new(addr: SocketAddr, client_id: u64) -> Self {
        let now = Instant::now();
        Self {
            addr,
            state: ConnectionState::Disconnected,
            client_id,
            local_sequence: 0,
            remote_sequence: 0,
            ack_bits: 0,
            rtt_ms: 0,
            last_recv_time: now,
            last_send_time: now,
            timeout: Duration::from_secs(10),
        }
    }

    /// Get the next sequence number.
    pub fn next_sequence(&mut self) -> u32 {
        let seq = self.local_sequence;
        self.local_sequence = self.local_sequence.wrapping_add(1);
        seq
    }

    /// Process a received sequence number.
    pub fn process_ack(&mut self, sequence: u32) {
        if sequence > self.remote_sequence {
            // Shift ack bits
            let diff = sequence - self.remote_sequence;
            if diff < 32 {
                self.ack_bits = (self.ack_bits << diff) | 1;
            } else {
                self.ack_bits = 1;
            }
            self.remote_sequence = sequence;
        } else if sequence < self.remote_sequence {
            let diff = self.remote_sequence - sequence;
            if diff < 32 {
                self.ack_bits |= 1 << diff;
            }
        }
        self.last_recv_time = Instant::now();
    }

    /// Check if the connection has timed out.
    #[must_use]
    pub fn is_timed_out(&self) -> bool {
        self.last_recv_time.elapsed() > self.timeout
    }

    /// Check if we should send a keepalive.
    #[must_use]
    pub fn needs_keepalive(&self) -> bool {
        self.last_send_time.elapsed() > Duration::from_millis(500)
    }

    /// Mark that we sent a packet.
    pub fn mark_sent(&mut self) {
        self.last_send_time = Instant::now();
    }
}
