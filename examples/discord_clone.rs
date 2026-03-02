use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use webrtc_core::engine_handle::EngineHandle;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr: SocketAddr = "127.0.0.1:9000".parse()?;
    let peer: SocketAddr = "127.0.0.1:9001".parse()?;
    let socket = UdpSocket::bind(addr).await?;
    let socket = Arc::new(socket);
    let handle = EngineHandle::builder().build();
    let rtcp_task = handle.start_rtcp_sender(socket.clone(), peer);
    let payload = vec![0u8; 160];
    for i in 0..100u32 {
        let _ = handle.feed_packet(&payload, i as u16, 0x1234);
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let _ = rtcp_task.await;
    Ok(())
}
