use std::sync::Arc;
use webrtc_core::e2ee::{SFrameContext, SFrameConfig, KeyStore, SFrameError};

fn main() {
    println!("=== SFrame End-to-End Encryption Example ===\n");

    let store = Arc::new(KeyStore::new());
    let key = [0x42u8; 32];
    let salt = [0x01u8; 12];
    store.add_key(0, &key, salt);
    println!("Added key 0 to keystore");

    let alice_ctx = SFrameContext::new(SFrameConfig::default(), store.clone(), 0);
    println!("Created Alice's SFrame context with key ID 0");

    store.add_key(1, &[0x24u8; 32], [0x02u8; 12]);
    let bob_ctx = SFrameContext::new(SFrameConfig::default(), store.clone(), 0);
    println!("Created Bob's SFrame context with key ID 0");

    let rtp_header = [
        0x80u8, 0x60, 0x00, 0x01,
        0x00, 0x00, 0x03, 0xE8,
        0x00, 0x00, 0x04, 0xD2,
    ];
    let plaintext = b"Hello from Alice! This is a confidential video frame payload.";

    println!("\n=== Encryption ===");
    println!("RTP header: {} bytes", rtp_header.len());
    println!("Plaintext payload: {} bytes", plaintext.len());

    let mut encrypted = vec![0u8; 512];
    let enc_len = alice_ctx.encrypt_frame(&rtp_header, plaintext, &mut encrypted).unwrap();
    println!("Encrypted frame: {} bytes ({} overhead)", 
        enc_len, 
        enc_len - plaintext.len() - rtp_header.len()
    );

    println!("\n=== Decryption ===");
    let mut to_decrypt = encrypted[..enc_len].to_vec();
    let plain_len = bob_ctx.decrypt_frame(&mut to_decrypt, rtp_header.len()).unwrap();
    println!("Decrypted payload: {} bytes", plain_len);
    
    let sframe_header_len = 1 + 4;
    let decrypted_start = rtp_header.len() + sframe_header_len;
    let decrypted_payload = &to_decrypt[decrypted_start..decrypted_start + plain_len];
    assert_eq!(decrypted_payload, plaintext);
    println!("Decryption verified: payload matches original!");

    println!("\n=== Error Handling ===");
    let store_no_key = Arc::new(KeyStore::new());
    let ctx_no_key = SFrameContext::new(SFrameConfig::default(), store_no_key, 99);
    let mut buf = vec![0u8; 256];
    let result = ctx_no_key.encrypt_frame(&rtp_header, b"test", &mut buf);
    match result {
        Err(SFrameError::KeyNotFound(kid)) => println!("Correctly got KeyNotFound error for kid={}", kid),
        _ => println!("Unexpected result: {:?}", result),
    }

    println!("\n=== Key Management ===");
    println!("Keys in keystore: {}", store.key_count());
    store.remove_key(0);
    println!("After removing key 0: {} keys", store.key_count());

    println!("\n=== SFrame Example Complete ===");
}
