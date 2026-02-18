//! Network client implementation.

use crate::connection::{Connection, ConnectionState};
use crate::packet::{NetPacket, PacketType};
use crate::transport::{Transport, TransportError};
use std::net::SocketAddr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("Not connected")]
    NotConnected,
    #[error("Connection failed")]
    ConnectionFailed,
    #[error("Connection timed out")]
    Timeout,
}

/// Network client for connecting to a game host.
pub struct NetClient {
    transport: Transport,
    connection: Option<Connection>,
    host_addr: Option<SocketAddr>,
}

impl NetClient {
    /// Create a new client.
    pub fn new() -> Self {
        Self {
            transport: Transport::new(),
            connection: None,
            host_addr: None,
        }
    }

    /// Connect to a host.
    pub async fn connect(&mut self, host: SocketAddr) -> Result<(), ClientError> {
        // Bind to any available port
        self.transport.bind("0.0.0.0:0".parse().unwrap()).await?;

        self.host_addr = Some(host);

        // Create connection state
        let mut conn = Connection::new(host, 0);
        conn.state = ConnectionState::Connecting;

        // Send connect request
        let packet = NetPacket::new(PacketType::Connect, conn.next_sequence());
        self.transport.send_to(&packet, host).await?;
        conn.mark_sent();

        self.connection = Some(conn);
        tracing::info!("Connecting to {}", host);

        Ok(())
    }

    /// Disconnect from the host.
    pub async fn disconnect(&mut self) -> Result<(), ClientError> {
        if let (Some(conn), Some(host)) = (&mut self.connection, self.host_addr) {
            let packet = NetPacket::new(PacketType::Disconnect, conn.next_sequence());
            self.transport.send_to(&packet, host).await?;
            conn.state = ConnectionState::Disconnected;
            tracing::info!("Disconnected from {}", host);
        }
        self.connection = None;
        self.host_addr = None;
        Ok(())
    }

    /// Process incoming packets.
    pub async fn poll(&mut self) -> Result<Vec<NetPacket>, ClientError> {
        let mut received = Vec::new();

        loop {
            match self.transport.try_recv_from() {
                Ok(Some((packet, _addr))) => {
                    if let Some(conn) = &mut self.connection {
                        conn.process_ack(packet.sequence);

                        // Handle connect ack
                        if packet.packet_type == PacketType::ConnectAck
                            && conn.state == ConnectionState::Connecting
                        {
                            conn.state = ConnectionState::Connected;
                            tracing::info!("Connected to host");
                        }
                    }
                    received.push(packet);
                }
                Ok(None) => break,
                Err(e) => return Err(ClientError::Transport(e)),
            }
        }

        // Check for timeout
        if let Some(conn) = &self.connection {
            if conn.is_timed_out() {
                return Err(ClientError::Timeout);
            }
        }

        Ok(received)
    }

    /// Send a packet to the host.
    pub async fn send(&mut self, mut packet: NetPacket) -> Result<(), ClientError> {
        let conn = self.connection.as_mut().ok_or(ClientError::NotConnected)?;
        let host = self.host_addr.ok_or(ClientError::NotConnected)?;

        packet.sequence = conn.next_sequence();
        packet.ack = conn.remote_sequence;
        packet.ack_bits = conn.ack_bits;

        self.transport.send_to(&packet, host).await?;
        conn.mark_sent();
        Ok(())
    }

    /// Check if connected.
    pub fn is_connected(&self) -> bool {
        self.connection
            .as_ref()
            .map(|c| c.state == ConnectionState::Connected)
            .unwrap_or(false)
    }

    /// Get the connection state.
    pub fn state(&self) -> ConnectionState {
        self.connection
            .as_ref()
            .map(|c| c.state)
            .unwrap_or(ConnectionState::Disconnected)
    }

    /// Get the local address.
    pub fn local_addr(&self) -> Result<SocketAddr, ClientError> {
        Ok(self.transport.local_addr()?)
    }
}

impl Default for NetClient {
    fn default() -> Self {
        Self::new()
    }
}
