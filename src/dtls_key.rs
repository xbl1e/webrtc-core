use ring::hkdf::{Salt, HKDF_SHA256};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum KeyDeriveError { #[error("insufficient input")] Insufficient }

pub fn derive_srtp_master_and_salt(extracted: &[u8]) -> Result<([u8;32],[u8;12]), KeyDeriveError> {
    if extracted.len() < 16 { return Err(KeyDeriveError::Insufficient); }
    let salt = Salt::new(HKDF_SHA256, extracted);
    let prk = salt.extract(&[]);
    let info_key: &[&[u8]] = &[b"EXTRACTOR-SRTP-KEY"];
    let info_salt: &[&[u8]] = &[b"EXTRACTOR-SRTP-SALT"];
    let okm_k = prk.expand(info_key, HKDF_SHA256).map_err(|_| KeyDeriveError::Insufficient)?;
    let okm_s = prk.expand(info_salt, HKDF_SHA256).map_err(|_| KeyDeriveError::Insufficient)?;
    let mut key = [0u8;32];
    let mut s = [0u8;12];
    okm_k.fill(&mut key).map_err(|_| KeyDeriveError::Insufficient)?;
    okm_s.fill(&mut s).map_err(|_| KeyDeriveError::Insufficient)?;
    Ok((key,s))
}
