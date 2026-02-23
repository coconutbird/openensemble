//! Network host (server) implementation.

use crate::connection::{Connection, ConnectionState};
use crate::packet::{NetPacket, PacketType};
use crate::transport::{Transport, TransportError};
use std::collections::HashMap;
use std::net::SocketAddr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HostError {
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("Max clients reached")]
    MaxClients,
    #[error("Client not found: {0}")]
    ClientNotFound(u64),
}

/// Network host for multiplayer games.
pub struct NetHost {
    transport: Transport,
    connections: HashMap<u64, Connection>,
    addr_to_client: HashMap<SocketAddr, u64>,
    next_client_id: u64,
    max_clients: usize,
}

impl NetHost {
    /// Create a new host.
    pub fn new(max_clients: usize) -> Self {
        Self {
            transport: Transport::new(),
            connections: HashMap::new(),
            addr_to_client: HashMap::new(),
            next_client_id: 1,
            max_clients,
        }
    }

    /// Start hosting on the given address.
    pub async fn start(&mut self, addr: SocketAddr) -> Result<(), HostError> {
        self.transport.bind(addr).await?;
        tracing::info!("Host started on {}", addr);
        Ok(())
    }

    /// Get the local address.
    pub fn local_addr(&self) -> Result<SocketAddr, HostError> {
        Ok(self.transport.local_addr()?)
    }

    /// Process incoming packets.
    pub async fn poll(&mut self) -> Result<Vec<(u64, NetPacket)>, HostError> {
        let mut received = Vec::new();

        loop {
            match self.transport.try_recv_from() {
                Ok(Some((packet, addr))) => {
                    if let Some(client_id) = self.handle_packet(packet.clone(), addr).await? {
                        received.push((client_id, packet));
                    }
                }
                Ok(None) => break,
                Err(e) => return Err(HostError::Transport(e)),
            }
        }

        Ok(received)
    }

    /// Handle an incoming packet.
    async fn handle_packet(
        &mut self,
        packet: NetPacket,
        addr: SocketAddr,
    ) -> Result<Option<u64>, HostError> {
        match packet.packet_type {
            PacketType::Connect => {
                let client_id = self.accept_connection(addr).await?;
                Ok(Some(client_id))
            }
            PacketType::Disconnect => {
                if let Some(&client_id) = self.addr_to_client.get(&addr) {
                    self.disconnect_client(client_id);
                    Ok(Some(client_id))
                } else {
                    Ok(None)
                }
            }
            _ => {
                if let Some(&client_id) = self.addr_to_client.get(&addr) {
                    if let Some(conn) = self.connections.get_mut(&client_id) {
                        conn.process_ack(packet.sequence);
                    }
                    Ok(Some(client_id))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Accept a new connection.
    async fn accept_connection(&mut self, addr: SocketAddr) -> Result<u64, HostError> {
        if self.connections.len() >= self.max_clients {
            return Err(HostError::MaxClients);
        }

        let client_id = self.next_client_id;
        self.next_client_id += 1;

        let mut conn = Connection::new(addr, client_id);
        conn.state = ConnectionState::Connected;

        self.connections.insert(client_id, conn);
        self.addr_to_client.insert(addr, client_id);

        // Send connect ack
        let ack = NetPacket::new(PacketType::ConnectAck, 0);
        self.transport.send_to(&ack, addr).await?;

        tracing::info!("Client {} connected from {}", client_id, addr);
        Ok(client_id)
    }

    /// Disconnect a client.
    pub fn disconnect_client(&mut self, client_id: u64) {
        if let Some(conn) = self.connections.remove(&client_id) {
            self.addr_to_client.remove(&conn.addr);
            tracing::info!("Client {} disconnected", client_id);
        }
    }

    /// Send a packet to a specific client.
    pub async fn send_to(&mut self, client_id: u64, packet: NetPacket) -> Result<(), HostError> {
        let conn = self
            .connections
            .get_mut(&client_id)
            .ok_or(HostError::ClientNotFound(client_id))?;
        self.transport.send_to(&packet, conn.addr).await?;
        conn.mark_sent();
        Ok(())
    }

    /// Broadcast a packet to all clients.
    pub async fn broadcast(&mut self, packet: NetPacket) -> Result<(), HostError> {
        let addrs: Vec<_> = self.connections.values().map(|c| c.addr).collect();
        for addr in addrs {
            self.transport.send_to(&packet, addr).await?;
        }
        Ok(())
    }

    /// Get connected client count.
    pub fn client_count(&self) -> usize {
        self.connections.len()
    }
}
