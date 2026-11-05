//! Time-based one-time passwords (RFC 6238: HMAC-SHA1, 30 s, 6 digits),
//! compatible with Google Authenticator, Microsoft Authenticator, Authy.

use hmac::{Hmac, Mac};
use sha1::Sha1;

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn base32_encode(data: &[u8]) -> String {
    let mut out = String::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for &b in data {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            out.push(ALPHABET[((buf >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

pub fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for c in s.chars().filter(|c| !c.is_whitespace() && *c != '=') {
        let v = ALPHABET.iter().position(|a| *a as char == c.to_ascii_uppercase())? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            out.push(((buf >> (bits - 8)) & 0xff) as u8);
            bits -= 8;
        }
    }
    Some(out)
}

pub fn new_secret() -> String {
    use rand::RngCore;
    let mut k = [0u8; 20];
    rand::thread_rng().fill_bytes(&mut k);
    base32_encode(&k)
}

pub fn code_at(secret: &[u8], counter: u64) -> String {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("any key length");
    mac.update(&counter.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let off = (h[19] & 0x0f) as usize;
    let bin = ((h[off] as u32 & 0x7f) << 24) | ((h[off + 1] as u32) << 16) | ((h[off + 2] as u32) << 8) | h[off + 3] as u32;
    format!("{:06}", bin % 1_000_000)
}

/// Accepts the current code and one step either side (clock drift).
pub fn verify(secret_b32: &str, code: &str, unix_time: u64) -> bool {
    let code: String = code.chars().filter(|c| c.is_ascii_digit()).collect();
    let Some(secret) = base32_decode(secret_b32) else { return false };
    if code.len() != 6 {
        return false;
    }
    let step = unix_time / 30;
    [step.saturating_sub(1), step, step + 1].iter().any(|c| code_at(&secret, *c) == code)
}

pub fn uri(secret: &str, email: &str) -> String {
    let enc = |s: &str| {
        s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect::<String>()
    };
    format!("otpauth://totp/TEMISZ:{}?secret={secret}&issuer=TEMISZ&algorithm=SHA1&digits=6&period=30", enc(email))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6238_vectors() {
        // RFC 6238 appendix B, SHA-1 secret "12345678901234567890" (8-digit values, last 6 digits)
        let secret = b"12345678901234567890";
        assert_eq!(code_at(secret, 59 / 30), "287082");
        assert_eq!(code_at(secret, 1111111109 / 30), "081804");
        assert_eq!(code_at(secret, 2000000000 / 30), "279037");
    }

    #[test]
    fn base32_roundtrip_and_window() {
        let s = new_secret();
        let raw = base32_decode(&s).unwrap();
        assert_eq!(base32_encode(&raw), s);
        let now = 1_800_000_000;
        let c = code_at(&raw, now / 30);
        assert!(verify(&s, &c, now));
        assert!(verify(&s, &c, now + 30));
        assert!(!verify(&s, &c, now + 120));
        assert!(!verify(&s, "12345", now));
    }
}
