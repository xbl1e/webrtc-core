use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use super::candidate::IceCandidate;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IceRole {
    Controlling,
    Controlled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IceState {
    New,
    Gathering,
    Checking,
    Connected,
    Completed,
    Failed,
    Closed,
}

#[derive(Clone, Debug)]
pub struct IceCandidatePair {
    pub local: IceCandidate,
    pub remote: IceCandidate,
    pub priority: u64,
    pub state: CandidatePairState,
    pub nominated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidatePairState {
    Waiting,
    InProgress,
    Succeeded,
    Failed,
    Frozen,
}

impl IceCandidatePair {
    pub fn new(local: IceCandidate, remote: IceCandidate, controlling: bool) -> Self {
        let priority = Self::compute_pair_priority(local.priority, remote.priority, controlling);
        Self {
            local,
            remote,
            priority,
            state: CandidatePairState::Waiting,
            nominated: false,
        }
    }

    fn compute_pair_priority(local: u32, remote: u32, controlling: bool) -> u64 {
        let (g, d) = if controlling { (local as u64, remote as u64) } else { (remote as u64, local as u64) };
        let role_bit = if controlling { 1u64 } else { 0u64 };
        (1u64 << 32) * g.min(d) + 2 * g.max(d) + role_bit
    }
}

#[derive(Clone, Debug)]
pub struct IceAgentConfig {
    pub local_ufrag: [u8; 16],
    pub local_ufrag_len: usize,
    pub local_pwd: [u8; 24],
    pub local_pwd_len: usize,
    pub remote_ufrag: [u8; 16],
    pub remote_ufrag_len: usize,
    pub remote_pwd: [u8; 24],
    pub remote_pwd_len: usize,
    pub lite: bool,
    pub aggressive_nomination: bool,
}

impl IceAgentConfig {
    pub fn new() -> Self {
        Self {
            local_ufrag: [0u8; 16],
            local_ufrag_len: 0,
            local_pwd: [0u8; 24],
            local_pwd_len: 0,
            remote_ufrag: [0u8; 16],
            remote_ufrag_len: 0,
            remote_pwd: [0u8; 24],
            remote_pwd_len: 0,
            lite: false,
            aggressive_nomination: true,
        }
    }

    pub fn set_local_credentials(&mut self, ufrag: &str, pwd: &str) {
        let u = ufrag.as_bytes();
        let len = u.len().min(16);
        self.local_ufrag[..len].copy_from_slice(&u[..len]);
        self.local_ufrag_len = len;
        let p = pwd.as_bytes();
        let plen = p.len().min(24);
        self.local_pwd[..plen].copy_from_slice(&p[..plen]);
        self.local_pwd_len = plen;
    }

    pub fn set_remote_credentials(&mut self, ufrag: &str, pwd: &str) {
        let u = ufrag.as_bytes();
        let len = u.len().min(16);
        self.remote_ufrag[..len].copy_from_slice(&u[..len]);
        self.remote_ufrag_len = len;
        let p = pwd.as_bytes();
        let plen = p.len().min(24);
        self.remote_pwd[..plen].copy_from_slice(&p[..plen]);
        self.remote_pwd_len = plen;
    }

    pub fn local_ufrag_str(&self) -> &str {
        std::str::from_utf8(&self.local_ufrag[..self.local_ufrag_len]).unwrap_or("")
    }

    pub fn local_pwd_str(&self) -> &str {
        std::str::from_utf8(&self.local_pwd[..self.local_pwd_len]).unwrap_or("")
    }
}

impl Default for IceAgentConfig {
    fn default() -> Self {
        Self::new()
    }
}

pub struct IceAgent {
    cfg: IceAgentConfig,
    role: IceRole,
    state: Mutex<IceState>,
    local_candidates: Mutex<Vec<IceCandidate>>,
    remote_candidates: Mutex<Vec<IceCandidate>>,
    candidate_pairs: Mutex<Vec<IceCandidatePair>>,
    selected_pair: Mutex<Option<IceCandidatePair>>,
    nominated_pair: Mutex<Option<IceCandidatePair>>,
    tie_breaker: u64,
    checks_sent: AtomicU32,
    gathering_done: AtomicBool,
    pending_events: Mutex<VecDeque<IceEvent>>,
}

#[derive(Clone, Debug)]
pub enum IceEvent {
    CandidateGathered(IceCandidate),
    GatheringDone,
    CandidatePairSelected(IceCandidatePair),
    Connected(SocketAddr),
    Failed,
    StateChanged(IceState),
}

impl IceAgent {
    pub fn new(cfg: IceAgentConfig, role: IceRole) -> Self {
        let tie_breaker = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        Self {
            cfg,
            role,
            state: Mutex::new(IceState::New),
            local_candidates: Mutex::new(Vec::new()),
            remote_candidates: Mutex::new(Vec::new()),
            candidate_pairs: Mutex::new(Vec::new()),
            selected_pair: Mutex::new(None),
            nominated_pair: Mutex::new(None),
            tie_breaker,
            checks_sent: AtomicU32::new(0),
            gathering_done: AtomicBool::new(false),
            pending_events: Mutex::new(VecDeque::new()),
        }
    }

    pub fn gather_host_candidates(&self, addresses: &[SocketAddr]) {
        let mut candidates = self.local_candidates.lock().unwrap();
        let mut events = self.pending_events.lock().unwrap();
        for &addr in addresses {
            let mut c = IceCandidate::new_host(addr, 1);
            c.set_foundation(&format!("host{}", addr.port()));
            c.set_ufrag(self.cfg.local_ufrag_str());
            events.push_back(IceEvent::CandidateGathered(c));
            candidates.push(c);
        }
        self.gathering_done.store(true, Ordering::Release);
        events.push_back(IceEvent::GatheringDone);
        *self.state.lock().unwrap() = IceState::Gathering;
    }

    pub fn add_remote_candidate(&self, candidate: IceCandidate) {
        let local = self.local_candidates.lock().unwrap();
        let is_controlling = self.role == IceRole::Controlling;
        let mut pairs = self.candidate_pairs.lock().unwrap();
        for lc in local.iter() {
            let pair = IceCandidatePair::new(lc.clone(), candidate.clone(), is_controlling);
            pairs.push(pair);
        }
        pairs.sort_by(|a, b| b.priority.cmp(&a.priority));
        let mut remote = self.remote_candidates.lock().unwrap();
        remote.push(candidate);
        let mut state = self.state.lock().unwrap();
        if *state == IceState::Gathering || *state == IceState::New {
            *state = IceState::Checking;
        }
    }

    pub fn checks_sent(&self) -> u32 {
        self.checks_sent.load(Ordering::Relaxed)
    }

    pub fn perform_connectivity_check(&self, remote_addr: SocketAddr) {
        self.checks_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn is_connected(&self) -> bool {
        self.state() == IceState::Connected || self.state() == IceState::Completed
    }

    pub fn selected_address(&self) -> Option<SocketAddr> {
        self.nominated_pair.lock().unwrap().as_ref().map(|p| p.remote.address)
    }

    pub fn local_candidate_count(&self) -> usize {
        self.local_candidates.lock().unwrap().len()
    }

    pub fn remote_candidate_count(&self) -> usize {
        self.remote_candidates.lock().unwrap().len()
    }

    pub fn poll_event(&self) -> Option<IceEvent> {
        self.pending_events.lock().unwrap().pop_front()
    }

    pub fn role(&self) -> IceRole {
        self.role
    }

    pub fn tie_breaker(&self) -> u64 {
        self.tie_breaker
    }
}

impl Default for IceAgent {
    fn default() -> Self {
        Self::new(IceAgentConfig::new(), IceRole::Controlling)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ice_agent_gather() {
        let mut cfg = IceAgentConfig::new();
        cfg.set_local_credentials("abcdefgh", "passwordpasswordpassword");
        cfg.set_remote_credentials("ijklmnop", "remotepasswordremote00");
        let agent = IceAgent::new(cfg, IceRole::Controlling);
        let addrs: Vec<SocketAddr> = vec!["127.0.0.1:5000".parse().unwrap()];
        agent.gather_host_candidates(&addrs);
        assert_eq!(agent.local_candidate_count(), 1);
        let remote = IceCandidate::new_host("127.0.0.1:6000".parse().unwrap(), 1);
        agent.add_remote_candidate(remote);
        assert_eq!(agent.state(), IceState::Checking);
        agent.perform_connectivity_check("127.0.0.1:6000".parse().unwrap());
        assert_eq!(agent.checks_sent(), 1);
    }

    #[test]
    fn candidate_pair_priority_controlling() {
        let local = IceCandidate::new_host("127.0.0.1:5000".parse().unwrap(), 1);
        let remote = IceCandidate::new_host("127.0.0.1:6000".parse().unwrap(), 1);
        let pair_ctrl = IceCandidatePair::new(local.clone(), remote.clone(), true);
        let pair_ctrd = IceCandidatePair::new(local, remote, false);
        assert_ne!(pair_ctrl.priority, pair_ctrd.priority);
    }
}
