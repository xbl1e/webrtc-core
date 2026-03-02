use crate::srtp::SrtpContext;
use std::sync::Arc;

pub enum SessionState {
    Unprotected,
    Protected(Arc<SrtpContext>),
}

impl SessionState {
    pub fn new() -> Self {
        SessionState::Unprotected
    }
    pub fn protect(self, key: &[u8; 32]) -> Self {
        SessionState::Protected(Arc::new(SrtpContext::new(key)))
    }
    pub fn is_protected(&self) -> bool {
        matches!(self, SessionState::Protected(_))
    }
    pub fn srtp(&self) -> Option<Arc<SrtpContext>> {
        match self {
            SessionState::Protected(a) => Some(a.clone()),
            _ => None,
        }
    }
}
