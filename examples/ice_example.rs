use std::net::SocketAddr;
use webrtc_core::ice::{IceAgent, IceAgentConfig, IceRole, IceCandidate, CandidateType};

fn main() {
    println!("=== ICE Agent Example ===\n");

    let mut cfg = IceAgentConfig::new();
    cfg.set_local_credentials("local_ufrag_1234", "local_password_12345678");
    cfg.set_remote_credentials("remote_ufrag_5678", "remote_password_87654321");

    let agent = IceAgent::new(cfg, IceRole::Controlling);
    println!("Created ICE agent as Controlling");

    let local_addrs: Vec<SocketAddr> = vec![
        "192.168.1.100:5000".parse().unwrap(),
        "10.0.0.1:5001".parse().unwrap(),
    ];
    agent.gather_host_candidates(&local_addrs);
    println!("Gathered {} local host candidates", agent.local_candidate_count());

    let remote = IceCandidate::new_host("192.168.1.200:6000".parse().unwrap(), 1);
    agent.add_remote_candidate(remote);
    println!("Added remote candidate");

    println!("ICE state: {:?}", agent.state());

    agent.simulate_successful_check();
    println!("Simulated successful connectivity check");

    if agent.is_connected() {
        println!("ICE connection established!");
        if let Some(addr) = agent.selected_address() {
            println!("Selected remote address: {}", addr);
        }
    }

    println!("\n=== Candidate SDP Attributes ===");
    let mut candidate = IceCandidate::new_host("192.168.1.100:5000".parse().unwrap(), 1);
    candidate.set_foundation("host1");
    println!("Host candidate: {}", candidate.to_sdp_attribute());

    let srflx = IceCandidate::new_srflx(
        "203.0.113.1:5000".parse().unwrap(),
        "192.168.1.100:5000".parse().unwrap(),
        1,
    );
    println!("SRFLX candidate: {}", srflx.to_sdp_attribute());

    println!("\n=== Candidate Priority Ordering ===");
    let host_prio = IceCandidate::compute_priority(CandidateType::Host, 65535, 1);
    let srflx_prio = IceCandidate::compute_priority(CandidateType::ServerReflexive, 65535, 1);
    let relay_prio = IceCandidate::compute_priority(CandidateType::Relay, 65535, 1);
    println!("Host priority: {}", host_prio);
    println!("SRFLX priority: {}", srflx_prio);
    println!("Relay priority: {}", relay_prio);
    assert!(host_prio > srflx_prio && srflx_prio > relay_prio);
    println!("Priority ordering verified: Host > SRFLX > Relay");

    println!("\n=== ICE Example Complete ===");
}
