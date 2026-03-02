use std::net::SocketAddr;
use webrtc_core::ice::{IceAgent, IceAgentConfig, IceRole, IceCandidate, CandidateType};

fn main() {
    println!("ICE Agent Example\n");

    let mut cfg = IceAgentConfig::new();
    cfg.set_local_credentials("local_ufrag_1234", "local_password_12345678");
    cfg.set_remote_credentials("remote_ufrag_5678", "remote_password_87654321");

    let agent = IceAgent::new(cfg, IceRole::Controlling);
    println!("Role: Controlling");

    let local_addrs: Vec<SocketAddr> = vec![
        "192.168.1.100:5000".parse().unwrap(),
        "10.0.0.1:5001".parse().unwrap(),
    ];
    agent.gather_host_candidates(&local_addrs);
    println!("Local candidates: {}", agent.local_candidate_count());

    let remote = IceCandidate::new_host("192.168.1.200:6000".parse().unwrap(), 1);
    agent.add_remote_candidate(remote);
    println!("State: {:?}", agent.state());

    agent.simulate_successful_check();
    println!("Connected: {}", agent.is_connected());
    
    if let Some(addr) = agent.selected_address() {
        println!("Selected: {}", addr);
    }

    println!("\nCandidate SDP:");
    let mut candidate = IceCandidate::new_host("192.168.1.100:5000".parse().unwrap(), 1);
    candidate.set_foundation("host1");
    println!("{}", candidate.to_sdp_attribute());

    let srflx = IceCandidate::new_srflx(
        "203.0.113.1:5000".parse().unwrap(),
        "192.168.1.100:5000".parse().unwrap(),
        1,
    );
    println!("{}", srflx.to_sdp_attribute());

    println!("\nPriority ordering:");
    let host = IceCandidate::compute_priority(CandidateType::Host, 65535, 1);
    let srflx = IceCandidate::compute_priority(CandidateType::ServerReflexive, 65535, 1);
    let relay = IceCandidate::compute_priority(CandidateType::Relay, 65535, 1);
    println!("Host: {}", host);
    println!("SRFLX: {}", srflx);
    println!("Relay: {}", relay);
}
