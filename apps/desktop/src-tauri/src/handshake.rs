//! The handshake between two Rusty Business™ installs on your network.
//!
//! Brad, 2026-11-25: "one [Rusty Business™] can talk to another [Rusty
//! Business™] on the same LAN, it has it's own connection handshake", and
//! "anything bubbled back ... goes to rb to rb back to me". Then, 2026-11-27:
//! "The handshake".
//!
//! One install pairs with another's courier door the way a courier app does:
//! with the six-digit code the other window shows. From then on the key the
//! door gave never crosses the network again. Both sides hold the same
//! secret, the key's SHA-256, and prove they hold it:
//!
//! - Each request is signed. The asking install sends its id, a fresh random
//!   number, the time, and an HMAC-SHA256 over all three and the body's
//!   SHA-256. The door turns away a signature that does not hold, a time more
//!   than two minutes off, and a number it has seen before, so a request
//!   copied off the network cannot be sent again, and its question cannot be
//!   changed.
//! - Each answer proves itself. The door's answer carries an HMAC over the
//!   asker's number, which only an install holding the secret could make. An
//!   answer without it, or with a wrong one, is never shown.
//!
//! HMAC-SHA256 is written here on `sha2`, which the app already builds, and
//! checked against RFC 4231, so no package is added for it.

use sha2::{Digest, Sha256};

/// The header a signed request carries.
pub const SIGNED: &str = "Rusty-Business-Install";
/// The header a door's answer proves itself with.
pub const PROOF: &str = "Rusty-Business-Proof";
/// How far apart the two clocks may be, in seconds.
pub const CLOCK_SLACK: u64 = 120;

const ASK_LABEL: &str = "rusty-business ask v1";
const ANSWER_LABEL: &str = "rusty-business answer v1";

/// HMAC-SHA256, as RFC 2104 and RFC 4231 lay it out.
pub fn hmac(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > block.len() {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block.map(|byte| byte ^ 0x36));
    inner.update(message);
    let mut outer = Sha256::new();
    outer.update(block.map(|byte| byte ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The secret both installs hold: the SHA-256 of the key the door gave, as
/// lowercase hex. The door keeps it in place of the key; the asking install
/// keeps it and forgets the key.
pub fn secret_of(key: &str) -> String {
    hex(&Sha256::digest(key.as_bytes()))
}

/// Equal, in the same time however soon the two differ.
pub fn same(one: &[u8], other: &[u8]) -> bool {
    one.len() == other.len()
        && one
            .iter()
            .zip(other)
            .fold(0u8, |differ, (a, b)| differ | (a ^ b))
            == 0
}

fn is_hex64(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn ask_mac(secret: &str, id: &str, nonce: &str, time: u64, body: &[u8]) -> String {
    let message = format!(
        "{ASK_LABEL}\n{id}\n{nonce}\n{time}\n{}",
        hex(&Sha256::digest(body))
    );
    hex(&hmac(secret.as_bytes(), message.as_bytes()))
}

/// The signed header's value for one request.
pub fn sign(secret: &str, id: &str, nonce: &str, time: u64, body: &[u8]) -> String {
    let mac = ask_mac(secret, id, nonce, time, body);
    format!("id={id}, nonce={nonce}, time={time}, mac={mac}")
}

/// A signed header, read but not yet checked.
#[derive(Debug, PartialEq)]
pub struct Signed {
    pub id: String,
    pub nonce: String,
    pub time: u64,
    mac: String,
}

/// Reads a signed header. Anything not in its exact shape is nothing.
pub fn read(value: &str) -> Option<Signed> {
    let mut parts = value.split(", ");
    let mut take = |name: &str| {
        parts
            .next()?
            .strip_prefix(name)?
            .strip_prefix('=')
            .map(str::to_string)
    };
    let signed = Signed {
        id: take("id")?,
        nonce: take("nonce")?,
        time: take("time")?.parse().ok()?,
        mac: take("mac")?,
    };
    if parts.next().is_some()
        || signed.id.is_empty()
        || signed.id.len() > 64
        || !signed
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || !is_hex64(&signed.nonce)
        || !is_hex64(&signed.mac)
    {
        return None;
    }
    Some(signed)
}

/// True when the signature holds for this secret and body, and its time is
/// within the slack of `now`. Whether its number was seen before is the
/// door's to check.
pub fn holds(signed: &Signed, secret: &str, body: &[u8], now: u64) -> bool {
    let expected = ask_mac(secret, &signed.id, &signed.nonce, signed.time, body);
    same(expected.as_bytes(), signed.mac.as_bytes()) && now.abs_diff(signed.time) <= CLOCK_SLACK
}

/// The proof a door's answer carries, for the asker's number.
pub fn proof(secret: &str, nonce: &str) -> String {
    let message = format!("{ANSWER_LABEL}\n{nonce}");
    hex(&hmac(secret.as_bytes(), message.as_bytes()))
}

/// True when an answer's proof is the one only the other install could make.
pub fn proven(secret: &str, nonce: &str, said: Option<&str>) -> bool {
    said.is_some_and(|said| same(proof(secret, nonce).as_bytes(), said.trim().as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_is_the_one_rfc_4231_gives() {
        // Test cases 1, 2, and 6 of RFC 4231, for HMAC-SHA-256.
        assert_eq!(
            hex(&hmac(&[0x0b; 20], b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hex(&hmac(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_eq!(
            hex(&hmac(
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn a_signature_holds_only_for_its_secret_its_body_and_its_time() {
        let secret = secret_of("the key the door gave");
        let nonce = "a".repeat(64);
        let header = sign(&secret, "one-id", &nonce, 1_000, b"{\"q\":1}");
        let signed = read(&header).unwrap();
        assert_eq!(signed.id, "one-id");
        assert!(holds(&signed, &secret, b"{\"q\":1}", 1_000));
        assert!(holds(&signed, &secret, b"{\"q\":1}", 1_000 + CLOCK_SLACK));
        assert!(holds(&signed, &secret, b"{\"q\":1}", 1_000 - CLOCK_SLACK));
        // A changed question, another secret, or a clock too far off.
        assert!(!holds(&signed, &secret, b"{\"q\":2}", 1_000));
        assert!(!holds(
            &signed,
            &secret_of("another key"),
            b"{\"q\":1}",
            1_000
        ));
        assert!(!holds(
            &signed,
            &secret,
            b"{\"q\":1}",
            1_000 + CLOCK_SLACK + 1
        ));
        // The key itself is not the secret.
        assert!(!holds(
            &signed,
            "the key the door gave",
            b"{\"q\":1}",
            1_000
        ));
    }

    #[test]
    fn a_signed_header_is_read_only_in_its_exact_shape() {
        let nonce = "b".repeat(64);
        let mac = "c".repeat(64);
        let good = format!("id=x1, nonce={nonce}, time=5, mac={mac}");
        assert!(read(&good).is_some());
        for bad in [
            format!("id=x1, nonce={nonce}, time=5"),
            format!("id=x1, nonce={nonce}, time=5, mac={mac}, more=1"),
            format!("nonce={nonce}, id=x1, time=5, mac={mac}"),
            format!("id=, nonce={nonce}, time=5, mac={mac}"),
            format!("id=x 1, nonce={nonce}, time=5, mac={mac}"),
            format!("id=x1, nonce=short, time=5, mac={mac}"),
            format!("id=x1, nonce={nonce}, time=soon, mac={mac}"),
            format!("id=x1, nonce={}, time=5, mac={mac}", "B".repeat(64)),
        ] {
            assert!(read(&bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn an_answer_is_proven_only_by_the_secret_and_the_askers_number() {
        let secret = secret_of("k");
        let nonce = "d".repeat(64);
        let said = proof(&secret, &nonce);
        assert!(proven(&secret, &nonce, Some(&said)));
        assert!(!proven(&secret, &"e".repeat(64), Some(&said)));
        assert!(!proven(&secret_of("other"), &nonce, Some(&said)));
        assert!(!proven(&secret, &nonce, None));
        assert!(!proven(&secret, &nonce, Some("")));
        // An answer's proof is not a request's signature.
        let header = sign(&secret, "id", &nonce, 1, b"");
        assert!(!proven(
            &secret,
            &nonce,
            read(&header).map(|s| s.mac).as_deref()
        ));
    }
}
