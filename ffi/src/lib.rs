use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::sync::Arc;
use crate::pc::{PeerConnection, RtcConfiguration, MediaKind};
use crate::ice::{IceAgent, IceAgentConfig, IceRole};
use crate::dtls::{DtlsEndpoint, DtlsRole};
use crate::sctp::SctpTransport;
use crate::datachannel::{DataChannel, DataChannelManager, DataChannelConfig, DataChannelState};

#[repr(C)]
pub struct WcPeerConnection {
    _private: [u8; 0],
}

#[repr(C)]
pub struct WcDataChannel {
    _private: [u8; 0],
}

#[repr(C)]
pub struct WcIceCandidate {
    _private: [u8; 0],
}

#[repr(C)]
pub struct WcSessionDescription {
    _private: [u8; 0],
}

#[repr(C)]
pub struct WcMediaStream {
    _private: [u8; 0],
}

#[repr(C)]
pub struct WcStatsReport {
    _private: [u8; 0],
}

pub type WcOnIceCandidateCallback = extern "C" fn(*mut c_void, *const c_char);
pub type WcOnDataChannelCallback = extern "C" fn(*mut c_void, *mut WcDataChannel);
pub type WcOnConnectionStateCallback = extern "C" fn(*mut c_void, c_int);
pub type WcOnSignalingStateCallback = extern "C" fn(*mut c_void, c_int);
pub type WcOnTrackCallback = extern "C" fn(*mut c_void, *mut WcMediaStream);
pub type WcOnDataChannelMessageCallback = extern "C" fn(*mut c_void, *mut WcDataChannel, *const c_char, c_int);

#[no_mangle]
pub extern "C" fn wc_peer_connection_create(config_json: *const c_char) -> *mut WcPeerConnection {
    let cfg = if config_json.is_null() {
        RtcConfiguration::default()
    } else {
        unsafe {
            let c_str = CStr::from_ptr(config_json);
            let json_str = c_str.to_string_lossy();
            RtcConfiguration::default()
        }
    };
    
    let pc = PeerConnection::new(cfg);
    let boxed = Box::new(pc);
    Box::into_raw(boxed) as *mut WcPeerConnection
}

#[no_mangle]
pub extern "C" fn wc_peer_connection_free(pc: *mut WcPeerConnection) {
    if !pc.is_null() {
        unsafe {
            let pc = Box::from_raw(pc as *mut PeerConnection);
            pc.close();
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_peer_connection_create_offer(pc: *mut WcPeerConnection) -> *mut WcSessionDescription {
    if pc.is_null() {
        return std::ptr::null_mut();
    }
    
    unsafe {
        let pc = &*(pc as *mut PeerConnection);
        match pc.create_offer() {
            Ok(sdp) => {
                let boxed = Box::new(sdp);
                Box::into_raw(boxed) as *mut WcSessionDescription
            }
            Err(_) => std::ptr::null_mut(),
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_peer_connection_create_answer(pc: *mut WcPeerConnection) -> *mut WcSessionDescription {
    if pc.is_null() {
        return std::ptr::null_mut();
    }
    
    unsafe {
        let pc = &*(pc as *mut PeerConnection);
        match pc.create_answer() {
            Ok(sdp) => {
                let boxed = Box::new(sdp);
                Box::into_raw(boxed) as *mut WcSessionDescription
            }
            Err(_) => std::ptr::null_mut(),
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_peer_connection_set_local_description(
    pc: *mut WcPeerConnection,
    sdp_type: c_int,
    sdp: *const c_char,
) -> c_int {
    if pc.is_null() || sdp.is_null() {
        return -1;
    }
    
    unsafe {
        let pc = &*(pc as *mut PeerConnection);
        let c_str = CStr::from_ptr(sdp);
        let sdp_str = c_str.to_string_lossy();
        
        let sdp_type = match sdp_type {
            0 => crate::pc::SdpType::Offer,
            1 => crate::pc::SdpType::Answer,
            2 => crate::pc::SdpType::PrAnswer,
            _ => return -1,
        };
        
        let desc = crate::pc::SessionDescription {
            sdp_type,
            sdp: sdp_str.to_string(),
        };
        
        match pc.set_local_description(desc) {
            Ok(_) => 0,
            Err(_) => -1,
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_peer_connection_set_remote_description(
    pc: *mut WcPeerConnection,
    sdp_type: c_int,
    sdp: *const c_char,
) -> c_int {
    if pc.is_null() || sdp.is_null() {
        return -1;
    }
    
    unsafe {
        let pc = &*(pc as *mut PeerConnection);
        let c_str = CStr::from_ptr(sdp);
        let sdp_str = c_str.to_string_lossy();
        
        let sdp_type = match sdp_type {
            0 => crate::pc::SdpType::Offer,
            1 => crate::pc::SdpType::Answer,
            2 => crate::pc::SdpType::PrAnswer,
            _ => return -1,
        };
        
        let desc = crate::pc::SessionDescription {
            sdp_type,
            sdp: sdp_str.to_string(),
        };
        
        match pc.set_remote_description(desc) {
            Ok(_) => 0,
            Err(_) => -1,
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_peer_connection_add_transceiver(
    pc: *mut WcPeerConnection,
    kind: c_int,
) -> *mut c_void {
    if pc.is_null() {
        return std::ptr::null_mut();
    }
    
    unsafe {
        let pc = &*(pc as *mut PeerConnection);
        let media_kind = match kind {
            0 => MediaKind::Audio,
            1 => MediaKind::Video,
            _ => return std::ptr::null_mut(),
        };
        
        let tc = pc.add_transceiver(media_kind);
        let ptr = Arc::into_raw(tc) as *mut c_void;
        ptr
    }
}

#[no_mangle]
pub extern "C" fn wc_peer_connection_create_data_channel(
    pc: *mut WcPeerConnection,
    label: *const c_char,
    ordered: c_int,
    max_retransmits: c_int,
) -> *mut WcDataChannel {
    if pc.is_null() || label.is_null() {
        return std::ptr::null_mut();
    }
    
    unsafe {
        let c_str = CStr::from_ptr(label);
        let label_str = c_str.to_string_lossy();
        
        let config = DataChannelConfig {
            label: label_str.to_string(),
            ordered: ordered != 0,
            max_retransmits: if max_retransmits >= 0 { Some(max_retransmits as u16) } else { None },
            ..Default::default()
        };
        
        let channel = DataChannel::new(0, config);
        let boxed = Box::new(channel);
        Box::into_raw(boxed) as *mut WcDataChannel
    }
}

#[no_mangle]
pub extern "C" fn wc_peer_connection_close(pc: *mut WcPeerConnection) {
    if !pc.is_null() {
        unsafe {
            let pc = &*(pc as *mut PeerConnection);
            pc.close();
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_peer_connection_get_state(pc: *mut WcPeerConnection) -> c_int {
    if pc.is_null() {
        return -1;
    }
    
    unsafe {
        let pc = &*(pc as *mut PeerConnection);
        match pc.state() {
            crate::pc::PeerConnectionState::New => 0,
            crate::pc::PeerConnectionState::Connecting => 1,
            crate::pc::PeerConnectionState::Connected => 2,
            crate::pc::PeerConnectionState::Disconnected => 3,
            crate::pc::PeerConnectionState::Failed => 4,
            crate::pc::PeerConnectionState::Closed => 5,
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_data_channel_free(channel: *mut WcDataChannel) {
    if !channel.is_null() {
        unsafe {
            Box::from_raw(channel as *mut DataChannel);
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_data_channel_send_text(
    channel: *mut WcDataChannel,
    message: *const c_char,
) -> c_int {
    if channel.is_null() || message.is_null() {
        return -1;
    }
    
    unsafe {
        let channel = &*(channel as *mut DataChannel);
        let c_str = CStr::from_ptr(message);
        let msg = c_str.to_string_lossy();
        
        match channel.send_text(&msg) {
            Ok(_) => 0,
            Err(_) => -1,
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_data_channel_send_binary(
    channel: *mut WcDataChannel,
    data: *const c_char,
    len: c_int,
) -> c_int {
    if channel.is_null() || data.is_null() || len <= 0 {
        return -1;
    }
    
    unsafe {
        let channel = &*(channel as *mut DataChannel);
        let slice = std::slice::from_raw_parts(data as *const u8, len as usize);
        
        match channel.send_binary(slice) {
            Ok(_) => 0,
            Err(_) => -1,
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_data_channel_get_state(channel: *mut WcDataChannel) -> c_int {
    if channel.is_null() {
        return -1;
    }
    
    unsafe {
        let channel = &*(channel as *mut DataChannel);
        match channel.state() {
            DataChannelState::Connecting => 0,
            DataChannelState::Open => 1,
            DataChannelState::Closing => 2,
            DataChannelState::Closed => 3,
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_data_channel_get_buffered_amount(channel: *mut WcDataChannel) -> c_int {
    if channel.is_null() {
        return -1;
    }
    
    unsafe {
        let channel = &*(channel as *mut DataChannel);
        channel.buffered_amount() as c_int
    }
}

#[no_mangle]
pub extern "C" fn wc_session_description_free(sdp: *mut WcSessionDescription) {
    if !sdp.is_null() {
        unsafe {
            Box::from_raw(sdp as *mut crate::pc::SessionDescription);
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_session_description_get_sdp(sdp: *mut WcSessionDescription) -> *mut c_char {
    if sdp.is_null() {
        return std::ptr::null_mut();
    }
    
    unsafe {
        let sdp = &*(sdp as *mut crate::pc::SessionDescription);
        CString::new(sdp.sdp.clone()).unwrap().into_raw()
    }
}

#[no_mangle]
pub extern "C" fn wc_session_description_get_type(sdp: *mut WcSessionDescription) -> c_int {
    if sdp.is_null() {
        return -1;
    }
    
    unsafe {
        let sdp = &*(sdp as *mut crate::pc::SessionDescription);
        match sdp.sdp_type {
            crate::pc::SdpType::Offer => 0,
            crate::pc::SdpType::Answer => 1,
            crate::pc::SdpType::PrAnswer => 2,
            crate::pc::SdpType::Rollback => 3,
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_ice_agent_create() -> *mut c_void {
    let cfg = IceAgentConfig::new();
    let agent = IceAgent::new(cfg, IceRole::Controlling);
    let boxed = Box::new(agent);
    Box::into_raw(boxed) as *mut c_void
}

#[no_mangle]
pub extern "C" fn wc_ice_agent_free(agent: *mut c_void) {
    if !agent.is_null() {
        unsafe {
            Box::from_raw(agent as *mut IceAgent);
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_ice_agent_gather_candidates(agent: *mut c_void, ip: *const c_char, port: c_int) {
    if agent.is_null() || ip.is_null() {
        return;
    }
    
    unsafe {
        let agent = &*(agent as *mut IceAgent);
        let c_str = CStr::from_ptr(ip);
        let ip_str = c_str.to_string_lossy();
        
        let addr: std::net::SocketAddr = format!("{}:{}", ip_str, port)
            .parse()
            .unwrap();
        
        agent.gather_host_candidates(&[addr]);
    }
}

#[no_mangle]
pub extern "C" fn wc_ice_agent_add_remote_candidate(agent: *mut c_void, ip: *const c_char, port: c_int) {
    if agent.is_null() || ip.is_null() {
        return;
    }
    
    unsafe {
        let agent = &*(agent as *mut IceAgent);
        let c_str = CStr::from_ptr(ip);
        let ip_str = c_str.to_string_lossy();
        
        let addr: std::net::SocketAddr = format!("{}:{}", ip_str, port)
            .parse()
            .unwrap();
        
        let candidate = crate::ice::IceCandidate::new_host(addr, 1);
        agent.add_remote_candidate(candidate);
    }
}

#[no_mangle]
pub extern "C" fn wc_ice_agent_get_state(agent: *mut c_void) -> c_int {
    if agent.is_null() {
        return -1;
    }
    
    unsafe {
        let agent = &*(agent as *mut IceAgent);
        match agent.state() {
            crate::ice::IceState::New => 0,
            crate::ice::IceState::Gathering => 1,
            crate::ice::IceState::Checking => 2,
            crate::ice::IceState::Connected => 3,
            crate::ice::IceState::Completed => 4,
            crate::ice::IceState::Failed => 5,
            crate::ice::IceState::Closed => 6,
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_ice_agent_is_connected(agent: *mut c_void) -> c_int {
    if agent.is_null() {
        return 0;
    }
    
    unsafe {
        let agent = &*(agent as *mut IceAgent);
        if agent.is_connected() { 1 } else { 0 }
    }
}

#[no_mangle]
pub extern "C" fn wc_dtls_endpoint_create(is_server: c_int) -> *mut c_void {
    let role = if is_server != 0 { DtlsRole::Server } else { DtlsRole::Client };
    let endpoint = DtlsEndpoint::new(role);
    let boxed = Box::new(endpoint);
    Box::into_raw(boxed) as *mut c_void
}

#[no_mangle]
pub extern "C" fn wc_dtls_endpoint_free(endpoint: *mut c_void) {
    if !endpoint.is_null() {
        unsafe {
            Box::from_raw(endpoint as *mut DtlsEndpoint);
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_dtls_endpoint_get_state(endpoint: *mut c_void) -> c_int {
    if endpoint.is_null() {
        return -1;
    }
    
    unsafe {
        let endpoint = &*(endpoint as *mut DtlsEndpoint);
        match endpoint.state() {
            crate::dtls::DtlsState::Closed => 0,
            crate::dtls::DtlsState::Connecting => 1,
            crate::dtls::DtlsState::Connected => 2,
            crate::dtls::DtlsState::Failed => 3,
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_sctp_transport_create() -> *mut c_void {
    let transport = SctpTransport::new();
    let boxed = Box::new(transport);
    Box::into_raw(boxed) as *mut c_void
}

#[no_mangle]
pub extern "C" fn wc_sctp_transport_free(transport: *mut c_void) {
    if !transport.is_null() {
        unsafe {
            Box::from_raw(transport as *mut SctpTransport);
        }
    }
}

#[no_mangle]
pub extern "C" fn wc_version() -> *const c_char {
    CString::new("0.7.0").unwrap().into_raw()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ice_agent_ffi() {
        let agent = wc_ice_agent_create();
        assert!(!agent.is_null());
        wc_ice_agent_free(agent);
    }

    #[test]
    fn dtls_endpoint_ffi() {
        let endpoint = wc_dtls_endpoint_create(0);
        assert!(!endpoint.is_null());
        wc_dtls_endpoint_free(endpoint);
    }

    #[test]
    fn sctp_transport_ffi() {
        let transport = wc_sctp_transport_create();
        assert!(!transport.is_null());
        wc_sctp_transport_free(transport);
    }

    #[test]
    fn version_ffi() {
        let version = wc_version();
        unsafe {
            let c_str = CStr::from_ptr(version);
            assert_eq!(c_str.to_string_lossy(), "0.7.0");
        }
    }
}
