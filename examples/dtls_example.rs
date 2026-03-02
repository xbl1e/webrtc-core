use webrtc_core::dtls::{DtlsEndpoint, DtlsRole, DtlsRecordHeader, DtlsCipherSuite, DtlsSecurityParameters};

fn main() {
    println!("DTLS Example\n");

    let mut client = DtlsEndpoint::new(DtlsRole::Client);
    let mut server = DtlsEndpoint::new(DtlsRole::Server);
    println!("Client role: {:?}", client.role());
    println!("Server role: {:?}", server.role());

    let server_random = [0x02u8; 32];
    server.set_remote_random([0x01u8; 32]);
    client.set_remote_random(server_random);

    let premaster = [0x42u8; 48];
    let master_c = client.compute_master_secret(premaster);
    let master_s = server.compute_master_secret(premaster);
    println!("\nMaster secrets match: {}", master_c == master_s);

    client.handshake_complete(master_c);
    server.handshake_complete(master_s);
    println!("Handshake complete");
    println!("Client state: {:?}", client.state());
    println!("Server state: {:?}", server.state());

    if let Some((key, salt)) = client.srtp_keying_material() {
        println!("\nSRTP keying material:");
        println!("Key: {} bytes", key.len());
        println!("Salt: {} bytes", salt.len());
    }

    println!("\nEncryption:");
    let plaintext = b"Hello, DTLS!";
    let mut encrypted = vec![0u8; 256];
    let enc_len = client.encrypt(plaintext, &mut encrypted).unwrap();
    println!("Plaintext: {} bytes", plaintext.len());
    println!("Encrypted: {} bytes", enc_len);

    let hdr = DtlsRecordHeader::parse(&encrypted).unwrap();
    println!("Epoch: {}, Seq: {}", hdr.epoch, hdr.sequence_number);

    let record = &encrypted[..13 + hdr.length as usize];
    let mut decrypted = vec![0u8; 256];
    let dec_len = server.decrypt(record, &mut decrypted).unwrap();
    println!("\nDecrypted: {} bytes", dec_len);
    assert_eq!(&decrypted[..dec_len], plaintext);
    println!("Verified");
}
