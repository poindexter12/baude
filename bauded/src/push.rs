//! Web Push: VAPID (RFC 8292) + aes128gcm message encryption (RFC 8291 /
//! RFC 8188), implemented on pure-Rust crypto so the container needs no
//! system TLS libs. Subscriptions persist in the config dir; dead ones are
//! pruned when the push service answers 404/410.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aes_gcm::aead::Aead;
use aes_gcm::{Aes128Gcm, KeyInit, Nonce};
use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use hkdf::Hkdf;
use p256::ecdh::diffie_hellman;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::{PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

const VAPID_FILE: &str = "daemon-vapid.json";
const SUBS_FILE: &str = "daemon-push.json";

fn config_base() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("baude")
}

// ---- VAPID ----

#[derive(Serialize, Deserialize)]
struct VapidOnDisk {
    private: String, // base64url scalar
    public: String,  // base64url uncompressed point
}

#[derive(Clone)]
pub struct Vapid {
    key: SecretKey,
    pub public_b64: String,
}

impl Vapid {
    /// Load the daemon's VAPID keypair, generating one on first run.
    pub fn load_or_generate() -> Result<Vapid> {
        let path = config_base().join(VAPID_FILE);
        if let Ok(text) = std::fs::read_to_string(&path) {
            let disk: VapidOnDisk = serde_json::from_str(&text).context("bad vapid file")?;
            let bytes = B64.decode(&disk.private).context("bad vapid key")?;
            let key = SecretKey::from_slice(&bytes).map_err(|e| anyhow!("vapid key: {e}"))?;
            return Ok(Vapid {
                public_b64: disk.public,
                key,
            });
        }
        let key = SecretKey::random(&mut rand_core::OsRng);
        let public = key.public_key().to_encoded_point(false);
        let disk = VapidOnDisk {
            private: B64.encode(key.to_bytes()),
            public: B64.encode(public.as_bytes()),
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(&disk)?)?;
        Ok(Vapid {
            public_b64: disk.public,
            key,
        })
    }

    /// `Authorization: vapid t=<jwt>, k=<pub>` for one push-service origin.
    fn auth_header(&self, endpoint: &str) -> Result<String> {
        let aud = origin_of(endpoint)?;
        let exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + 12 * 3600;
        let header = B64.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = B64.encode(
            serde_json::json!({ "aud": aud, "exp": exp, "sub": "mailto:bauded@localhost" })
                .to_string(),
        );
        let signing: SigningKey = SigningKey::from(&self.key);
        let sig: Signature = signing.sign(format!("{header}.{claims}").as_bytes());
        let jwt = format!("{header}.{claims}.{}", B64.encode(sig.to_bytes()));
        Ok(format!("vapid t={jwt}, k={}", self.public_b64))
    }
}

fn origin_of(endpoint: &str) -> Result<String> {
    let rest = endpoint
        .strip_prefix("https://")
        .ok_or_else(|| anyhow!("push endpoint must be https"))?;
    let host = rest.split('/').next().unwrap_or(rest);
    Ok(format!("https://{host}"))
}

// ---- aes128gcm encryption (RFC 8291 + RFC 8188) ----

/// Encrypt `payload` for a subscriber, with caller-supplied ephemeral key
/// and salt so the construction is testable.
fn encrypt_with(
    ua_public: &[u8],
    auth_secret: &[u8],
    ephemeral: &SecretKey,
    salt: &[u8; 16],
    payload: &[u8],
) -> Result<Vec<u8>> {
    let ua_pub = PublicKey::from_sec1_bytes(ua_public).map_err(|e| anyhow!("p256dh: {e}"))?;
    let as_pub = ephemeral.public_key().to_encoded_point(false);
    let shared = diffie_hellman(ephemeral.to_nonzero_scalar(), ua_pub.as_affine());

    // IKM = HKDF(salt=auth, ikm=ecdh, info="WebPush: info" || 0x00 || ua_pub || as_pub)
    let mut info = Vec::with_capacity(144);
    info.extend_from_slice(b"WebPush: info\0");
    info.extend_from_slice(ua_public);
    info.extend_from_slice(as_pub.as_bytes());
    let hk = Hkdf::<Sha256>::new(Some(auth_secret), shared.raw_secret_bytes());
    let mut ikm = [0u8; 32];
    hk.expand(&info, &mut ikm)
        .map_err(|_| anyhow!("hkdf ikm"))?;

    // CEK + nonce from the record salt.
    let hk = Hkdf::<Sha256>::new(Some(salt), &ikm);
    let mut cek = [0u8; 16];
    hk.expand(b"Content-Encoding: aes128gcm\0", &mut cek)
        .map_err(|_| anyhow!("hkdf cek"))?;
    let mut nonce = [0u8; 12];
    hk.expand(b"Content-Encoding: nonce\0", &mut nonce)
        .map_err(|_| anyhow!("hkdf nonce"))?;

    // Single record: payload || 0x02 delimiter, AES-128-GCM.
    let mut record = payload.to_vec();
    record.push(0x02);
    let cipher = Aes128Gcm::new_from_slice(&cek).map_err(|_| anyhow!("cek"))?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), record.as_slice())
        .map_err(|_| anyhow!("encrypt"))?;

    // aes128gcm header: salt(16) | rs(4) | idlen(1) | keyid(as_pub, 65)
    let mut body = Vec::with_capacity(86 + ciphertext.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&4096u32.to_be_bytes());
    body.push(65);
    body.extend_from_slice(as_pub.as_bytes());
    body.extend_from_slice(&ciphertext);
    Ok(body)
}

pub fn encrypt(ua_public: &[u8], auth_secret: &[u8], payload: &[u8]) -> Result<Vec<u8>> {
    let ephemeral = SecretKey::random(&mut rand_core::OsRng);
    let mut salt = [0u8; 16];
    rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut salt);
    encrypt_with(ua_public, auth_secret, &ephemeral, &salt, payload)
}

// ---- subscriptions ----

#[derive(Serialize, Deserialize, Clone)]
pub struct Subscription {
    pub endpoint: String,
    pub p256dh: String, // base64url uncompressed point
    pub auth: String,   // base64url 16-byte secret
}

pub struct PushState {
    pub vapid: Vapid,
    subs: Vec<Subscription>,
    persist: bool,
}

pub type SharedPush = Arc<Mutex<PushState>>;

pub fn lock(shared: &SharedPush) -> MutexGuard<'_, PushState> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

impl PushState {
    pub fn load(persist: bool) -> Result<PushState> {
        let subs = if persist {
            std::fs::read_to_string(config_base().join(SUBS_FILE))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(PushState {
            vapid: Vapid::load_or_generate()?,
            subs,
            persist,
        })
    }

    fn save(&self) {
        if !self.persist {
            return;
        }
        let path = config_base().join(SUBS_FILE);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.subs) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn subscribe(&mut self, sub: Subscription) {
        self.subs.retain(|s| s.endpoint != sub.endpoint);
        self.subs.push(sub);
        self.save();
    }

    pub fn unsubscribe(&mut self, endpoint: &str) -> bool {
        let before = self.subs.len();
        self.subs.retain(|s| s.endpoint != endpoint);
        let removed = self.subs.len() != before;
        if removed {
            self.save();
        }
        removed
    }

    pub fn remove_dead(&mut self, dead: &[String]) {
        if dead.is_empty() {
            return;
        }
        self.subs.retain(|s| !dead.contains(&s.endpoint));
        self.save();
    }

    pub fn subs(&self) -> Vec<Subscription> {
        self.subs.clone()
    }
}

/// Send one payload to one subscription. Ok(false) = the subscription is
/// gone (404/410) and should be dropped.
pub fn send(vapid: &Vapid, sub: &Subscription, payload: &[u8]) -> Result<bool> {
    let ua_public = B64
        .decode(&sub.p256dh)
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(&sub.p256dh))
        .context("bad p256dh")?;
    let auth = B64
        .decode(&sub.auth)
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(&sub.auth))
        .context("bad auth secret")?;
    let body = encrypt(&ua_public, &auth, payload)?;
    let resp = ureq::post(&sub.endpoint)
        .timeout(Duration::from_secs(10))
        .set("Authorization", &vapid.auth_header(&sub.endpoint)?)
        .set("Content-Encoding", "aes128gcm")
        .set("TTL", "300")
        .set("Urgency", "high")
        .send_bytes(&body);
    match resp {
        Ok(_) => Ok(true),
        Err(ureq::Error::Status(404 | 410, _)) => Ok(false),
        Err(e) => bail!("push: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes_gcm::aead::Aead;

    /// Decrypt an aes128gcm body the way a user agent would — proves both
    /// directions of the RFC 8291 construction agree.
    fn decrypt(ua_key: &SecretKey, auth_secret: &[u8], body: &[u8]) -> Vec<u8> {
        let salt = &body[..16];
        let keyid_len = body[20] as usize;
        let as_pub_bytes = &body[21..21 + keyid_len];
        let ciphertext = &body[21 + keyid_len..];

        let ua_pub = ua_key.public_key().to_encoded_point(false);
        let as_pub = PublicKey::from_sec1_bytes(as_pub_bytes).unwrap();
        let shared = diffie_hellman(ua_key.to_nonzero_scalar(), as_pub.as_affine());

        let mut info = Vec::new();
        info.extend_from_slice(b"WebPush: info\0");
        info.extend_from_slice(ua_pub.as_bytes());
        info.extend_from_slice(as_pub_bytes);
        let hk = Hkdf::<Sha256>::new(Some(auth_secret), shared.raw_secret_bytes());
        let mut ikm = [0u8; 32];
        hk.expand(&info, &mut ikm).unwrap();

        let hk = Hkdf::<Sha256>::new(Some(salt), &ikm);
        let mut cek = [0u8; 16];
        hk.expand(b"Content-Encoding: aes128gcm\0", &mut cek)
            .unwrap();
        let mut nonce = [0u8; 12];
        hk.expand(b"Content-Encoding: nonce\0", &mut nonce).unwrap();

        let cipher = Aes128Gcm::new_from_slice(&cek).unwrap();
        let mut record = cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext)
            .unwrap();
        assert_eq!(record.pop(), Some(0x02), "last-record delimiter");
        record
    }

    #[test]
    fn encrypt_round_trips() {
        let ua_key = SecretKey::random(&mut rand_core::OsRng);
        let ua_pub = ua_key.public_key().to_encoded_point(false);
        let auth: [u8; 16] = [7; 16];
        let msg = br#"{"title":"api is waiting"}"#;

        let body = encrypt(ua_pub.as_bytes(), &auth, msg).unwrap();
        // header sanity: salt(16) + rs(4) + idlen(1) + key(65)
        assert_eq!(&body[16..20], &4096u32.to_be_bytes());
        assert_eq!(body[20], 65);
        assert_eq!(decrypt(&ua_key, &auth, &body), msg);
    }

    #[test]
    fn vapid_header_shape() {
        let key = SecretKey::random(&mut rand_core::OsRng);
        let public = key.public_key().to_encoded_point(false);
        let vapid = Vapid {
            public_b64: B64.encode(public.as_bytes()),
            key,
        };
        let header = vapid
            .auth_header("https://web.push.apple.com/QOX9...")
            .unwrap();
        assert!(header.starts_with("vapid t="));
        let jwt = header
            .strip_prefix("vapid t=")
            .unwrap()
            .split(',')
            .next()
            .unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let claims: serde_json::Value =
            serde_json::from_slice(&B64.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://web.push.apple.com");
        assert_eq!(B64.decode(parts[2]).unwrap().len(), 64); // raw r||s
    }
}
