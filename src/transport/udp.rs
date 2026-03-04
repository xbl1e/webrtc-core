//! UDP transport module for async network operations.
//!
//! Provides async UDP socket handling integrated with Tokio for
//! WebRTC ICE, DTLS, and media transport.

use std::net::{SocketAddr, IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, broadcast};
use parking_lot::RwLock;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("socket already bound")]
    AlreadyBound,
    #[error("not bound")]
    NotBound,
    #[error("channel closed")]
    ChannelClosed,
}
#[derive(Debug, Clone)]
pub struct IncomingPacket {
    pub data: Arc<Vec<u8>>,
    pub source: SocketAddr,
}
pub struct UdpEndpoint {
    socket: Arc<UdpSocket>,
    local_addr: SocketAddr,
    // Broadcast channel for incoming packets - allows multiple consumers
    incoming_tx: broadcast::Sender<IncomingPacket>,
    // Control channel for shutdown
    shutdown_tx: Arc<RwLock<Option<broadcast::Sender<()>>>>,
}

impl UdpEndpoint {
    pub async fn bind(addr: SocketAddr) -> Result<Self, TransportError> {
        let socket = UdpSocket::bind(addr).await?;
        let local_addr = socket.local_addr()?;

        // Create broadcast channel for incoming packets
        let (incoming_tx, _) = broadcast::channel(1024);

        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            socket: Arc::new(socket),
            local_addr,
            incoming_tx,
            shutdown_tx: Arc::new(RwLock::new(Some(shutdown_tx))),
        })
    }
    pub async fn bind_any(port: u16) -> Result<Self, TransportError> {
        Self::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port)).await
    }
    pub fn start_receiver(&self) {
        let socket = self.socket.clone();
        let mut incoming_rx = self.incoming_tx.subscribe();
        let shutdown_rx = {
            let tx = self.shutdown_tx.read();
            tx.as_ref().map(|t| t.subscribe())
        };

        tokio::spawn(async move {
            let mut buf = [0u8; 65536];
            loop {
                tokio::select! {
                    result = socket.recv_from(&mut buf) => {
                        match result {
                            Ok((len, src)) => {
                                let data = Arc::new(buf[..len].to_vec());
                                let packet = IncomingPacket { data, source: src };
                                if incoming_rx.send(packet).is_err() {
                                    // No receivers left
                                    break;
                                }
                            }
                            Err(e) => {
                                log::error!("UDP receive error: {}", e);
                                break;
                            }
                        }
                    }
                    _ = async {
                        if let Some(ref mut rx) = shutdown_rx {
                            rx.recv().await.ok()
                        } else {
                            None
                        }
                    } => {
                        break;
                    }
                }
            }
        });
    }
    pub async fn send_to(&self, data: &[u8], dest: SocketAddr) -> Result<usize, TransportError> {
        let len = self.socket.send_to(data, dest).await?;
        Ok(len)
    }
    pub async fn send_to_many(&self, data: &[u8], dests: &[SocketAddr]) -> Result<(), TransportError> {
        for dest in dests {
            self.socket.send_to(data, *dest).await?;
        }
        Ok(())
    }
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
    pub fn subscribe(&self) -> broadcast::Receiver<IncomingPacket> {
        self.incoming_tx.subscribe()
    }
    pub fn socket(&self) -> &Arc<UdpSocket> {
        &self.socket
    }
    pub fn is_active(&self) -> bool {
        self.shutdown_tx.read().is_some()
    }
    pub async fn shutdown(&self) {
        let tx = self.shutdown_tx.write().take();
        if let Some(tx) = tx {
            let _ = tx.send(());
        }
    }
}

impl Drop for UdpEndpoint {
    fn drop(&mut self) {
        let tx = self.shutdown_tx.write().take();
        if let Some(tx) = tx {
            let _ = tx.send(());
        }
    }
}
pub async fn socket_pair() -> Result<(UdpEndpoint, UdpEndpoint), TransportError> {
    let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
    let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);

    let sock1 = UdpSocket::bind(addr1).await?;
    let sock2 = UdpSocket::bind(addr2).await?;

    // Connect them
    sock1.connect(sock2.local_addr()?).await?;
    sock2.connect(sock1.local_addr()?).await?;

    let ep1 = UdpEndpoint {
        socket: Arc::new(sock1),
        local_addr: sock1.local_addr()?,
        incoming_tx: broadcast::channel(1024).0,
        shutdown_tx: Arc::new(RwLock::new(Some(broadcast::channel(1).0))),
    };

    let ep2 = UdpEndpoint {
        socket: Arc::new(sock2),
        local_addr: sock2.local_addr()?,
        incoming_tx: broadcast::channel(1024).0,
        shutdown_tx: Arc::new(RwLock::new(Some(broadcast::channel(1).0))),
    };

    Ok((ep1, ep2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn udp_endpoint_bind() {
        let endpoint = UdpEndpoint::bind_any(0).await.unwrap();
        assert!(endpoint.local_addr().port() != 0);
    }

    #[tokio::test]
    async fn udp_endpoint_send_receive() {
        let ep1 = UdpEndpoint::bind_any(0).await.unwrap();
        let ep2 = UdpEndpoint::bind_any(0).await.unwrap();

        ep1.start_receiver();
        ep2.start_receiver();

        let mut rx = ep1.subscribe();

        let data = b"hello world";
        ep2.send_to(data, ep1.local_addr()).await.unwrap();

        // Wait for packet
        let packet = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");

        assert_eq!(&packet.data[..], data);
        assert_eq!(packet.source, ep2.local_addr());
    }
}
