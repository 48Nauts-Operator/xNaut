// Layer 1 sealing: the argument blobs, encrypted at rest under a key the HSM
// holds.
//
// The chain (`evidence.rs`) proves what happened. It does not keep it private:
// a blob is the tool's real arguments, sitting on disk as plaintext. This seals
// them with AES-256-GCM under a per-session data key, and hands the 32 raw
// bytes of that key to the HSM to be encrypted under `xnaut-kek-v1` (TSB
// `POST /v1/encrypt`, AES_GCM, confirmed live 2026-08-20). The wrapped form is
// all that touches disk.
//
// Why not `/v1/wrap`: it takes `keyToBeWrapped` as a LABEL, so it only wraps
// keys already inside the HSM. Ours must never be one. Encrypting the key bytes
// under a KEK is wrapping in effect, and the content never leaves the machine.
//
// The property this buys, and it is the sellable one: delete
// `sealed/<session>.dek.json` and every blob in that session is unrecoverable
// by anyone, including us, while the chain still verifies end to end. Deleting
// rows from an append-only log cannot do that.
//
// Off unless `XNAUT_SEAL=1`. Sealing turns a local write into a network round
// trip, and an install with no TSB must keep recording.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{json, Value};

/// The KEK's label in TSB. Created 2026-08-20 as AES-256, encrypt+decrypt,
/// never extractable, uuid A0BACA8D-EF43A915-DA4C370B-DEED1C72.
const KEK_LABEL: &str = "xnaut-kek-v1";
const CIPHER: &str = "AES_GCM";

pub fn enabled() -> bool {
    std::env::var("XNAUT_SEAL").map(|v| v == "1").unwrap_or(false)
}

fn dek_path(session: &str) -> PathBuf {
    crate::evidence::dir().join("sealed").join(format!("{session}.dek.json"))
}

/// A shred leaves this behind. Without it a shredded session is
/// indistinguishable from one that was never sealed, and `read_blob` hands the
/// caller raw ciphertext as if it were the arguments. Garbage displayed as
/// evidence is worse than an error saying the key is gone.
fn tombstone_path(session: &str) -> PathBuf {
    crate::evidence::dir().join("sealed").join(format!("{session}.shredded"))
}

// ---- the local half ---------------------------------------------------------

/// `nonce || ciphertext||tag`. A fresh nonce per blob, because a repeat under
/// the same key is the one mistake GCM does not survive.
pub fn encrypt(dek: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let key = LessSafeKey::new(
        UnboundKey::new(&AES_256_GCM, dek).map_err(|_| "bad data key".to_string())?,
    );
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new().fill(&mut nonce).map_err(|_| "no entropy".to_string())?;
    let mut out = plaintext.to_vec();
    key.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::empty(), &mut out)
        .map_err(|_| "seal failed".to_string())?;
    let mut sealed = nonce.to_vec();
    sealed.extend_from_slice(&out);
    Ok(sealed)
}

pub fn decrypt(dek: &[u8; 32], sealed: &[u8]) -> Result<Vec<u8>, String> {
    if sealed.len() <= NONCE_LEN {
        return Err("truncated blob".into());
    }
    let key = LessSafeKey::new(
        UnboundKey::new(&AES_256_GCM, dek).map_err(|_| "bad data key".to_string())?,
    );
    let (nonce, body) = sealed.split_at(NONCE_LEN);
    let mut buf = body.to_vec();
    let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| "bad nonce".to_string())?;
    let clear = key
        .open_in_place(nonce, Aad::empty(), &mut buf)
        .map_err(|_| "blob does not verify under this key".to_string())?;
    Ok(clear.to_vec())
}

// ---- the HSM half -----------------------------------------------------------

/// TSB connection details, taken from the securosys-attest plugin's env so
/// there is one place the owner configures them. `load_store` has already
/// resolved the keychain sentinels.
struct Tsb {
    url: String,
    api_key: Option<String>,
    jwt: Option<String>,
}

fn tsb() -> Result<Tsb, String> {
    let env = crate::plugins::plugin_env("securosys-attest")
        .ok_or("the securosys-attest plugin is not in the library")?;
    let url = env
        .get("SECUROSYS_TSB_URL")
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .ok_or("SECUROSYS_TSB_URL is not set")?;
    Ok(Tsb {
        url,
        api_key: env.get("SECUROSYS_API_KEY").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        jwt: env.get("SECUROSYS_JWT").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
    })
}

fn post(tsb: &Tsb, path: &str, body: Value) -> Result<Value, String> {
    let mut req = reqwest::blocking::Client::new()
        .post(format!("{}{path}", tsb.url))
        .header("Content-Type", "application/json");
    if let Some(key) = &tsb.api_key {
        req = req.header("X-API-KEY", key);
    }
    if let Some(jwt) = &tsb.jwt {
        req = req.header("Authorization", format!("Bearer {jwt}"));
    }
    let response = req.json(&body).send().map_err(|e| format!("TSB unreachable: {e}"))?;
    let status = response.status();
    let text = response.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("TSB {status}: {text}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("TSB returned {e}"))
}

/// Encrypt the data key's bytes under the HSM-held KEK. The plaintext key goes
/// over the wire once and is never stored; the ciphertext is what lands on disk.
fn wrap(dek: &[u8; 32]) -> Result<Value, String> {
    let tsb = tsb()?;
    let answer = post(
        &tsb,
        "/v1/encrypt",
        json!({"encryptRequest": {
            "payload": STANDARD.encode(dek),
            "encryptKeyName": KEK_LABEL,
            "cipherAlgorithm": CIPHER,
        }}),
    )?;
    let ciphertext = answer["encryptedPayload"].as_str().ok_or("TSB returned no ciphertext")?;
    let iv = answer["initializationVector"].as_str().ok_or("TSB returned no IV")?;
    Ok(json!({
        "kek_label": KEK_LABEL,
        "cipher": CIPHER,
        "wrapped_dek": ciphertext,
        "initialization_vector": iv,
    }))
}

fn unwrap(wrapped: &Value) -> Result<[u8; 32], String> {
    let tsb = tsb()?;
    let answer = post(
        &tsb,
        "/v1/synchronousDecrypt",
        json!({"decryptRequest": {
            "cipherAlgorithm": wrapped["cipher"].as_str().unwrap_or(CIPHER),
            "decryptKeyName": wrapped["kek_label"].as_str().unwrap_or(KEK_LABEL),
            "encryptedPayload": wrapped["wrapped_dek"],
            "initializationVector": wrapped["initialization_vector"],
        }}),
    )?;
    let raw = STANDARD
        .decode(answer["payload"].as_str().ok_or("TSB returned no payload")?)
        .map_err(|e| e.to_string())?;
    raw.try_into().map_err(|_| "the unwrapped key is not 32 bytes".to_string())
}

// ---- the session key --------------------------------------------------------

fn cache() -> &'static Mutex<HashMap<String, [u8; 32]>> {
    static CACHE: OnceLock<Mutex<HashMap<String, [u8; 32]>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The session's data key: from memory, else unwrapped from disk, else fresh.
///
/// A fresh key is wrapped BEFORE it is used. Sealing a blob under a key the HSM
/// has never seen would produce a file nobody can ever open, which is data loss
/// wearing a security feature's clothes.
fn session_dek(session: &str) -> Result<[u8; 32], String> {
    if let Some(dek) = cache().lock().map_err(|_| "seal cache poisoned")?.get(session) {
        return Ok(*dek);
    }
    let path = dek_path(session);
    let dek = match std::fs::read_to_string(&path) {
        Ok(text) => {
            let wrapped: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            unwrap(&wrapped)?
        }
        Err(_) => {
            let mut fresh = [0u8; 32];
            SystemRandom::new().fill(&mut fresh).map_err(|_| "no entropy".to_string())?;
            let wrapped = wrap(&fresh)?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, serde_json::to_string_pretty(&wrapped).map_err(|e| e.to_string())?)
                .map_err(|e| format!("could not write {}: {e}", path.display()))?;
            fresh
        }
    };
    cache().lock().map_err(|_| "seal cache poisoned")?.insert(session.to_string(), dek);
    Ok(dek)
}

/// Write a blob, sealed if sealing is on. Returns nothing the record needs: the
/// record already names the blob by its hash, and that hash is over the
/// PLAINTEXT, so the chain reads the same whether the blob is sealed or not.
pub fn write_blob(session: &str, path: &Path, plaintext: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = if enabled() { encrypt(&session_dek(session)?, plaintext)? } else { plaintext.to_vec() };
    std::fs::write(path, body).map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// Read a blob back. Sealed or not is decided by what is on disk, not by the
/// current setting, so a bundle stays readable after the switch is flipped.
pub fn read_blob(session: &str, path: &Path) -> Result<Vec<u8>, String> {
    if tombstone_path(session).exists() {
        return Err(format!(
            "session {session} was shredded: its key is gone and this blob cannot be read by anyone"
        ));
    }
    let raw = std::fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    if !dek_path(session).exists() {
        return Ok(raw);
    }
    decrypt(&session_dek(session)?, &raw)
}

/// Does this session still hold a key, and was one ever destroyed?
///
/// Both flags, not one enum, because "never sealed" and "sealed then shredded"
/// are different facts about the same session and the panel says so differently.
pub fn state(session: &str) -> (bool, bool) {
    (dek_path(session).exists(), tombstone_path(session).exists())
}

/// Crypto-shredding: destroy the session's wrapped key. The blobs stay where
/// they are and become permanently unreadable; the chain over them does not
/// move, so everything still verifies.
pub fn shred(session: &str) -> Result<(), String> {
    cache().lock().map_err(|_| "seal cache poisoned")?.remove(session);
    let path = dek_path(session);
    let existed = path.exists();
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    if existed {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(tombstone_path(session), b"").map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dek(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    #[test]
    fn a_sealed_blob_comes_back_and_a_touched_one_does_not() {
        let key = dek(7);
        let sealed = encrypt(&key, b"rm -rf /Users/andre/Documents").unwrap();
        assert!(!sealed.windows(2).any(|w| w == b"rm"), "the plaintext is still on disk");
        assert_eq!(decrypt(&key, &sealed).unwrap(), b"rm -rf /Users/andre/Documents");

        // One flipped bit anywhere past the nonce must fail the tag, not
        // return plausible garbage.
        let mut tampered = sealed.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert!(decrypt(&key, &tampered).is_err(), "a modified blob opened anyway");

        assert!(decrypt(&dek(8), &sealed).is_err(), "the wrong key opened it");
    }

    #[test]
    fn the_same_plaintext_seals_differently_every_time() {
        // Equal ciphertexts would leak which two tool calls had equal
        // arguments, which for a shell command is most of the secret.
        let key = dek(3);
        assert_ne!(encrypt(&key, b"same").unwrap(), encrypt(&key, b"same").unwrap());
    }

    #[test]
    fn shredding_the_key_makes_the_blob_unrecoverable() {
        // The claim the product sells. Nothing here touches the HSM: the
        // wrapped key is written by hand so the local half is provable without
        // a network.
        let _lock = crate::evidence::DIR_LOCK.lock();
        let home = std::env::temp_dir().join(format!("xnaut-seal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("XNAUT_EVIDENCE_DIR", &home);
        let key = dek(11);
        let session = "shred-me";
        let blob = home.join("blobs").join(session).join("abc");
        std::fs::create_dir_all(blob.parent().unwrap()).unwrap();
        std::fs::write(&blob, encrypt(&key, b"the arguments").unwrap()).unwrap();
        std::fs::create_dir_all(dek_path(session).parent().unwrap()).unwrap();
        std::fs::write(dek_path(session), "{\"kek_label\":\"x\"}").unwrap();
        cache().lock().unwrap().insert(session.into(), key);

        assert_eq!(read_blob(session, &blob).unwrap(), b"the arguments");
        shred(session).unwrap();

        assert!(blob.exists(), "shredding must not delete the evidence, only the key");
        // The copy in memory counts. A key still cached after the file is gone
        // would keep sealing new blobs under something nobody can ever unwrap.
        assert!(!cache().lock().unwrap().contains_key(session), "the key is still in memory");
        // With the key gone the read must fail and say why. It used to fall
        // through to "no key means it was never sealed" and hand the caller
        // raw ciphertext, which the UI would have rendered as the arguments.
        let err = read_blob(session, &blob).expect_err("a shredded blob was read");
        assert!(err.contains("shredded"), "{err}");
        assert!(shred(session).is_ok(), "shredding twice must not fail");

        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// The only test that touches the HSM. `cargo test --bin xnaut -- --ignored
    /// seals_a_blob_against_the_real_hsm`.
    ///
    /// Everything above proves the local half against a key written by hand.
    /// This proves the half that matters commercially: the key xNAUT generated
    /// is one only the HSM can give back.
    #[test]
    #[ignore = "makes two live TSB calls; run with --ignored"]
    fn seals_a_blob_against_the_real_hsm() {
        let _lock = crate::evidence::DIR_LOCK.lock();
        let home = std::env::temp_dir().join(format!("xnaut-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("XNAUT_EVIDENCE_DIR", &home);
        std::env::set_var("XNAUT_SEAL", "1");
        let session = "live-hsm";
        let blob = crate::evidence::blob_path(session, "sha256:deadbeef");

        write_blob(session, &blob, b"cargo tauri build").expect("seal");
        let on_disk = std::fs::read(&blob).unwrap();
        assert!(!on_disk.windows(5).any(|w| w == b"cargo"), "plaintext reached the disk");

        // Force the unwrap path: drop the in-memory key so the only way back is
        // through the HSM.
        cache().lock().unwrap().remove(session);
        assert_eq!(read_blob(session, &blob).unwrap(), b"cargo tauri build");

        shred(session).unwrap();
        assert!(read_blob(session, &blob).unwrap_err().contains("shredded"), "shredding did not work");

        std::env::remove_var("XNAUT_SEAL");
        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&home);
    }
}
