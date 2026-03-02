use std::sync::Arc;
use webrtc_core::e2ee::{SFrameContext, SFrameConfig, KeyStore, SFrameError};

fn main() {
    println!("SFrame E2EE Example\n");

    let store = Arc::new(KeyStore::new());
    let key = [0x42u8; 32];
    let salt = [0x01u8; 12];
    store.add_key(0, &key, salt);
    println!("Key 0 added to keystore");

    let alice = SFrameContext::new(SFrameConfig::default(), store.clone(), 0);
    println!("Alice context: kid=0");

    store.add_key(1, &[0x24u8; 32], [0x02u8; 12]);
    let bob = SFrameContext::new(SFrameConfig::default(), store.clone(), 0);
    println!("Bob context: kid=0");

    let rtp_header = [
        0x80u8, 0x60, 0x00, 0x01,
        0x00, 0x00, 0x03, 0xE8,
        0x00, 0x00, 0x04, 0xD2,
    ];
    let plaintext = b"Hello from Alice!";

    println!("\nEncryption:");
    println!("Plaintext: {} bytes", plaintext.len());

    let mut encrypted = vec![0u8; 512];
    let enc_len = alice.encrypt_frame(&rtp_header, plaintext, &mut encrypted).unwrap();
    println!("Encrypted: {} bytes", enc_len);

    println!("\nDecryption:");
    let mut to_decrypt = encrypted[..enc_len].to_vec();
    let plain_len = bob.decrypt_frame(&mut to_decrypt, rtp_header.len()).unwrap();
    println!("Decrypted: {} bytes", plain_len);

    let sframe_hdr = 1 + 4;
    let start = rtp_header.len() + sframe_hdr;
    assert_eq!(&to_decrypt[start..start + plain_len], plaintext);
    println!("Verified: payload matches");

    println!("\nKey management:");
    println!("Keys: {}", store.key_count());
    store.remove_key(0);
    println!("After remove: {}", store.key_count());
}
