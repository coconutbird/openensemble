//! UDP transport layer.

use crate::packet::NetPacket;
use std::io;
use std::net::SocketAddr;
use thiserror::Error;
use tokio::net::UdpSocket;

/// Maximum packet size.
pub const MAX_PACKET_SIZE: usize = 1400;

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Packet too large: {size} > {max}")]
    PacketTooLarge { size: usize, max: usize },
    #[error("Socket not bound")]
    NotBound,
}

/// UDP transport for sending and receiving packets.
pub struct Transport {
    socket: Option<UdpSocket>,
    recv_buffer: Vec<u8>,
}

impl Transport {
    /// Create a new transport (not yet bound).
    pub fn new() -> Self {
        Self {
            socket: None,
            recv_buffer: vec![0u8; MAX_PACKET_SIZE],
        }
    }

    /// Bind to a local address.
    pub async fn bind(&mut self, addr: SocketAddr) -> Result<(), TransportError> {
        let socket = UdpSocket::bind(addr).await?;
        self.socket = Some(socket);
        Ok(())
    }

    /// Get the local address.
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        self.socket
            .as_ref()
            .ok_or(TransportError::NotBound)?
            .local_addr()
            .map_err(TransportError::Io)
    }

    /// Send a packet to a remote address.
    pub async fn send_to(
        &self,
        packet: &NetPacket,
        addr: SocketAddr,
    ) -> Result<usize, TransportError> {
        let socket = self.socket.as_ref().ok_or(TransportError::NotBound)?;
        let data = packet.serialize()?;

        if data.len() > MAX_PACKET_SIZE {
            return Err(TransportError::PacketTooLarge {
                size: data.len(),
                max: MAX_PACKET_SIZE,
            });
        }

        let sent = socket.send_to(&data, addr).await?;
        Ok(sent)
    }

    /// Receive a packet.
    pub async fn recv_from(&mut self) -> Result<(NetPacket, SocketAddr), TransportError> {
        let socket = self.socket.as_ref().ok_or(TransportError::NotBound)?;
        let (len, addr) = socket.recv_from(&mut self.recv_buffer).await?;
        let packet = NetPacket::deserialize(&self.recv_buffer[..len])?;
        Ok((packet, addr))
    }

    /// Try to receive a packet without blocking.
    pub fn try_recv_from(&mut self) -> Result<Option<(NetPacket, SocketAddr)>, TransportError> {
        let socket = self.socket.as_ref().ok_or(TransportError::NotBound)?;
        match socket.try_recv_from(&mut self.recv_buffer) {
            Ok((len, addr)) => {
                let packet = NetPacket::deserialize(&self.recv_buffer[..len])?;
                Ok(Some((packet, addr)))
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(TransportError::Io(e)),
        }
    }
}

impl Default for Transport {
    fn default() -> Self {
        Self::new()
    }
}
