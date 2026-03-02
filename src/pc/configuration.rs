#[derive(Clone, Debug)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

impl IceServer {
    pub fn stun(url: &str) -> Self {
        Self { urls: vec![url.to_string()], username: None, credential: None }
    }

    pub fn turn(url: &str, username: &str, credential: &str) -> Self {
        Self {
            urls: vec![url.to_string()],
            username: Some(username.to_string()),
            credential: Some(credential.to_string()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IceTransportPolicy {
    All,
    Relay,
}

impl Default for IceTransportPolicy {
    fn default() -> Self {
        IceTransportPolicy::All
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundlePolicy {
    Balanced,
    MaxBundle,
    MaxCompat,
}

impl Default for BundlePolicy {
    fn default() -> Self {
        BundlePolicy::MaxBundle
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtcpMuxPolicy {
    Require,
    Negotiate,
}

impl Default for RtcpMuxPolicy {
    fn default() -> Self {
        RtcpMuxPolicy::Require
    }
}

#[derive(Clone, Debug, Default)]
pub struct RtcConfiguration {
    pub ice_servers: Vec<IceServer>,
    pub ice_transport_policy: IceTransportPolicy,
    pub bundle_policy: BundlePolicy,
    pub rtcp_mux_policy: RtcpMuxPolicy,
    pub ice_candidate_pool_size: u8,
}

impl RtcConfiguration {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_stun(mut self, stun_url: &str) -> Self {
        self.ice_servers.push(IceServer::stun(stun_url));
        self
    }

    pub fn with_turn(mut self, turn_url: &str, username: &str, credential: &str) -> Self {
        self.ice_servers.push(IceServer::turn(turn_url, username, credential));
        self
    }

    pub fn has_turn(&self) -> bool {
        self.ice_servers.iter().any(|s| s.urls.iter().any(|u| u.starts_with("turn:")))
    }

    pub fn stun_servers(&self) -> Vec<&IceServer> {
        self.ice_servers.iter()
            .filter(|s| s.urls.iter().any(|u| u.starts_with("stun:")))
            .collect()
    }

    pub fn turn_servers(&self) -> Vec<&IceServer> {
        self.ice_servers.iter()
            .filter(|s| s.username.is_some())
            .collect()
    }
}
