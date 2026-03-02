use webrtc_core::dtls::{DtlsEndpoint, DtlsRole, DtlsState, DtlsRecordHeader, CONTENT_TYPE_APPLICATION_DATA, DTLS_VERSION_1_2};

fn main() {
    println!("=== DTLS Example ===\n");

    let mut client = DtlsEndpoint::new(DtlsRole::Client);
    let mut server = DtlsEndpoint::new(DtlsRole::Server);
    println!("Created DTLS client and server endpoints");

    println!("\nClient role: {:?}", client.role());
    println!("Server role: {:?}", server.role());

    println!("\nClient local random: {:02X?}", client.local_random());

    let server_random = [0x02u8; 32];
    server.set_remote_random([0x01u8; 32]);
    client.set_remote_random(server_random);

    let premaster = [0x42u8; 48];
    let master_client = client.compute_master_secret(premaster);
    let master_server = server.compute_master_secret(premaster);
    println!("\nComputed master secrets (client == server): {}", master_client == master_server);

    client.handshake_complete(master_client);
    server.handshake_complete(master_server);

    println!("\nHandshake complete!");
    println!("Client state: {:?}", client.state());
    println!("Server state: {:?}", server.state());
    println!("Client connected: {}", client.is_connected());
    println!("Server connected: {}", server.is_connected());

    if let Some((key, salt)) = client.srtp_keying_material() {
        println!("\nSRTP keying material:");
        println!("  Key (16 bytes): {:02X?}", key);
        println!("  Salt (14 bytes): {:02X?}", salt);
    }

    println!("\n=== DTLS Record Encryption ===");
    let plaintext = b"Hello, DTLS!";
    let mut encrypted = vec![0u8; 256];
    let enc_len = client.encrypt(plaintext, &mut encrypted).unwrap();
    println!("Plaintext: {} bytes", plaintext.len());
    println!("Encrypted: {} bytes", enc_len);

    let hdr = DtlsRecordHeader::parse(&encrypted).unwrap();
    println!("Record header:");
    println!("  Content type: {} (Application Data)", hdr.content_type);
    println!("  Version: {:#06X}", hdr.version);
    println!("  Epoch: {}", hdr.epoch);
    println!("  Sequence: {}", hdr.sequence_number);
    println!("  Length: {}", hdr.length);

    let record = &encrypted[..13 + hdr.length as usize];
    let mut decrypted = vec![0u8; 256];
    let dec_len = server.decrypt(record, &mut decrypted).unwrap();
    println!("\nDecrypted: {} bytes", dec_len);
    println!("Decrypted content: {}", String::from_utf8_lossy(&decrypted[..dec_len]));
    assert_eq!(&decrypted[..dec_len], plaintext);
    println!("Encryption/decryption verified!");

    println!("\n=== DTLS Example Complete ===");
}
