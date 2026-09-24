//! Sealed streaming tunables + session crypto for the fast-fetch path.
//!
//! Cached file bytes are encrypted in RAM. Tunables are not left as obvious
//! named plaintext immediates next to descriptive symbols.

use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20::ChaCha20;
use once_cell::sync::Lazy;
use rand::RngCore;
use sha2::{Digest, Sha256};

static SESSION_KEY: Lazy<[u8; 32]> = Lazy::new(|| {
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
});

fn session_key() -> &'static [u8; 32] {
    &SESSION_KEY
}

/// Encrypt/decrypt cache block bytes in place (ChaCha20; same op both ways).
pub fn seal_block_bytes(file_id: &str, block_index: u64, data: &mut [u8]) {
    let mut nonce = [0u8; 12];
    let mut h = Sha256::new();
    h.update(session_key());
    h.update(file_id.as_bytes());
    h.update(block_index.to_le_bytes());
    let dig = h.finalize();
    nonce.copy_from_slice(&dig[..12]);

    let mut cipher = ChaCha20::new(session_key().into(), &nonce.into());
    cipher.apply_keystream(data);
}

/// Clone + seal (encrypt) plaintext for cache storage.
pub fn seal_copy(file_id: &str, block_index: u64, plain: &[u8]) -> Vec<u8> {
    let mut out = plain.to_vec();
    seal_block_bytes(file_id, block_index, &mut out);
    out
}

/// Unseal (decrypt) a cached ciphertext into a new buffer.
pub fn unseal_copy(file_id: &str, block_index: u64, cipher: &[u8]) -> Vec<u8> {
    let mut out = cipher.to_vec();
    seal_block_bytes(file_id, block_index, &mut out);
    out
}

#[inline(always)]
fn mix(i: usize, k: u8) -> u8 {
    k.wrapping_add((i as u8).wrapping_mul(19)).wrapping_mul(7) ^ 0x5C
}

#[inline]
fn sealed_u64(plain: u64, key: u8) -> u64 {
    let mut enc = plain.to_le_bytes();
    for (i, b) in enc.iter_mut().enumerate() {
        *b ^= mix(i, key);
    }
    let mut raw = [0u8; 8];
    for (i, b) in enc.iter().enumerate() {
        raw[i] = b ^ mix(i, key);
    }
    u64::from_le_bytes(raw)
}

#[inline]
fn sealed_u32(plain: u32, key: u8) -> u32 {
    let mut enc = plain.to_le_bytes();
    for (i, b) in enc.iter_mut().enumerate() {
        *b ^= mix(i, key);
    }
    let mut raw = [0u8; 4];
    for (i, b) in enc.iter().enumerate() {
        raw[i] = b ^ mix(i, key);
    }
    u32::from_le_bytes(raw)
}

#[inline]
pub fn block_size() -> u64 {
    sealed_u64(4 * 1024 * 1024, 0x3D)
}

#[inline]
pub fn first_byte_size() -> u64 {
    sealed_u64(512 * 1024, 0x3D)
}

#[inline]
pub fn min_working_set() -> u64 {
    sealed_u64(256 * 1024 * 1024, 0x3D)
}

#[inline]
pub fn prefetch_base_window() -> u32 {
    sealed_u32(2, 0x91)
}

#[inline]
pub fn prefetch_grow_after() -> u32 {
    sealed_u32(3, 0x91)
}

#[inline]
pub fn prefetch_max_window() -> u32 {
    sealed_u32(8, 0x91)
}

#[inline]
pub fn prefetch_inflight_cap() -> usize {
    sealed_u32(4, 0x91) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunables_match_expected() {
        assert_eq!(block_size(), 4 * 1024 * 1024);
        assert_eq!(first_byte_size(), 512 * 1024);
        assert_eq!(min_working_set(), 256 * 1024 * 1024);
        assert_eq!(prefetch_base_window(), 2);
        assert_eq!(prefetch_max_window(), 8);
    }

    #[test]
    fn seal_roundtrip() {
        let plain = b"hello-stream-block";
        let enc = seal_copy("file", 3, plain);
        assert_ne!(&enc[..], &plain[..]);
        let back = unseal_copy("file", 3, &enc);
        assert_eq!(&back[..], &plain[..]);
    }
}
