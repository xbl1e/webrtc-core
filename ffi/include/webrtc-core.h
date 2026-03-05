#ifndef WEBRTC_CORE_H
#define WEBRTC_CORE_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct WcPeerConnection WcPeerConnection;
typedef struct WcDataChannel WcDataChannel;
typedef struct WcIceCandidate WcIceCandidate;
typedef struct WcSessionDescription WcSessionDescription;
typedef struct WcMediaStream WcMediaStream;
typedef struct WcStatsReport WcStatsReport;
typedef struct WcTransceiver WcTransceiver;

typedef void (*WcOnIceCandidateCallback)(void* user_data, const char* candidate);
typedef void (*WcOnDataChannelCallback)(void* user_data, WcDataChannel* channel);
typedef void (*WcOnConnectionStateCallback)(void* user_data, int state);
typedef void (*WcOnSignalingStateCallback)(void* user_data, int state);
typedef void (*WcOnTrackCallback)(void* user_data, WcMediaStream* stream);
typedef void (*WcOnDataChannelMessageCallback)(void* user_data, WcDataChannel* channel, const char* message, int len);

WcPeerConnection* wc_peer_connection_create(const char* config_json);
void wc_peer_connection_free(WcPeerConnection* pc);
WcSessionDescription* wc_peer_connection_create_offer(WcPeerConnection* pc);
WcSessionDescription* wc_peer_connection_create_answer(WcPeerConnection* pc);
int wc_peer_connection_set_local_description(WcPeerConnection* pc, int sdp_type, const char* sdp);
int wc_peer_connection_set_remote_description(WcPeerConnection* pc, int sdp_type, const char* sdp);
WcTransceiver* wc_peer_connection_add_transceiver(WcPeerConnection* pc, int kind);
void wc_transceiver_free(WcTransceiver* tr);
WcDataChannel* wc_peer_connection_create_data_channel(WcPeerConnection* pc, const char* label, int ordered, int max_retransmits);
void wc_peer_connection_close(WcPeerConnection* pc);
int wc_peer_connection_get_state(WcPeerConnection* pc);

void wc_data_channel_free(WcDataChannel* channel);
int wc_data_channel_send_text(WcDataChannel* channel, const char* message);
int wc_data_channel_send_binary(WcDataChannel* channel, const char* data, int len);
int wc_data_channel_get_state(WcDataChannel* channel);
int wc_data_channel_get_buffered_amount(WcDataChannel* channel);

void wc_session_description_free(WcSessionDescription* sdp);
char* wc_session_description_get_sdp(WcSessionDescription* sdp);
void wc_string_free(char* s);
int wc_session_description_get_type(WcSessionDescription* sdp);

void* wc_ice_agent_create(void);
void wc_ice_agent_free(void* agent);
void wc_ice_agent_gather_candidates(void* agent, const char* ip, int port);
void wc_ice_agent_add_remote_candidate(void* agent, const char* ip, int port);
int wc_ice_agent_get_state(void* agent);
int wc_ice_agent_is_connected(void* agent);

void* wc_dtls_endpoint_create(int is_server);
void wc_dtls_endpoint_free(void* endpoint);
int wc_dtls_endpoint_get_state(void* endpoint);

void* wc_sctp_transport_create(void);
void wc_sctp_transport_free(void* transport);

const char* wc_version(void);

#ifdef __cplusplus
}
#endif

#endif
