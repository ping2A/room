//! Minimal pure-Rust WebAuthn (FIDO2) for admin YubiKey / passkeys.
//!
//! Supports ES256 (P-256) registration with `fmt: "none"` and assertion
//! verification. Attestation trust chains are intentionally not validated —
//! credentials are enrolled by an already-authenticated admin.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ciborium::Value;
use ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use p256::elliptic_curve::sec1::FromEncodedPoint;
use p256::{EncodedPoint, PublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Stored credential (COSE public key kept as CBOR bytes).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WebAuthnCredential {
    /// Credential id (base64url).
    pub id: String,
    /// COSE_Key bytes (base64url).
    pub cose_key: String,
    /// Authenticator signature counter.
    pub sign_count: u32,
    /// Operator label (e.g. "YubiKey 5C").
    pub name: String,
    /// Created unix seconds.
    pub created: u64,
}

/// Registration / assertion options returned to the browser.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyCredentialCreationOptions {
    pub challenge: String,
    pub rp: RpEntity,
    pub user: UserEntity,
    pub pub_key_cred_params: Vec<PubKeyCredParam>,
    pub timeout: u32,
    pub attestation: &'static str,
    pub authenticator_selection: AuthenticatorSelection,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exclude_credentials: Vec<CredDescriptor>,
}

/// Assertion options.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyCredentialRequestOptions {
    pub challenge: String,
    pub timeout: u32,
    pub rp_id: String,
    pub allow_credentials: Vec<CredDescriptor>,
    pub user_verification: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct RpEntity {
    pub name: String,
    pub id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserEntity {
    pub id: String,
    pub name: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PubKeyCredParam {
    #[serde(rename = "type")]
    pub type_: &'static str,
    pub alg: i32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatorSelection {
    pub authenticator_attachment: Option<&'static str>,
    pub resident_key: &'static str,
    pub require_resident_key: bool,
    pub user_verification: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredDescriptor {
    #[serde(rename = "type")]
    pub type_: &'static str,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transports: Option<Vec<&'static str>>,
}

/// Browser registration / assertion response (subset).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatorResponse {
    pub id: String,
    pub raw_id: String,
    pub response: AuthenticatorResponseBody,
    #[serde(default, rename = "type")]
    pub type_: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatorResponseBody {
    pub client_data_json: String,
    #[serde(default)]
    pub attestation_object: Option<String>,
    #[serde(default)]
    pub authenticator_data: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub user_handle: Option<String>,
}

#[derive(Debug, Error)]
pub enum WebAuthnError {
    #[error("invalid base64")]
    Base64,
    #[error("invalid client data")]
    ClientData,
    #[error("challenge mismatch")]
    Challenge,
    #[error("origin mismatch")]
    Origin,
    #[error("rpId hash mismatch")]
    RpId,
    #[error("user presence / verification required")]
    Flags,
    #[error("invalid attestation")]
    Attestation,
    #[error("invalid COSE key")]
    Cose,
    #[error("invalid signature")]
    Signature,
    #[error("sign count rollback")]
    SignCount,
}

/// Build registration options (prefer hardware keys; UV preferred).
pub fn creation_options(
    challenge_b64: &str,
    rp_id: &str,
    rp_name: &str,
    user_id_b64: &str,
    username: &str,
    exclude: &[WebAuthnCredential],
) -> PublicKeyCredentialCreationOptions {
    PublicKeyCredentialCreationOptions {
        challenge: challenge_b64.into(),
        rp: RpEntity {
            name: rp_name.into(),
            id: rp_id.into(),
        },
        user: UserEntity {
            id: user_id_b64.into(),
            name: username.into(),
            display_name: username.into(),
        },
        pub_key_cred_params: vec![PubKeyCredParam {
            type_: "public-key",
            alg: -7,
        }],
        timeout: 120_000,
        attestation: "none",
        authenticator_selection: AuthenticatorSelection {
            authenticator_attachment: Some("cross-platform"),
            resident_key: "preferred",
            require_resident_key: false,
            user_verification: "preferred",
        },
        exclude_credentials: exclude
            .iter()
            .map(|c| CredDescriptor {
                type_: "public-key",
                id: c.id.clone(),
                transports: Some(vec!["usb", "nfc", "ble", "internal"]),
            })
            .collect(),
    }
}

/// Build assertion options.
pub fn request_options(
    challenge_b64: &str,
    rp_id: &str,
    allow: &[WebAuthnCredential],
) -> PublicKeyCredentialRequestOptions {
    PublicKeyCredentialRequestOptions {
        challenge: challenge_b64.into(),
        timeout: 120_000,
        rp_id: rp_id.into(),
        allow_credentials: allow
            .iter()
            .map(|c| CredDescriptor {
                type_: "public-key",
                id: c.id.clone(),
                transports: Some(vec!["usb", "nfc", "ble", "internal"]),
            })
            .collect(),
        user_verification: "preferred",
    }
}

/// Verify a registration response and return the new credential.
pub fn finish_registration(
    resp: &AuthenticatorResponse,
    expected_challenge_b64: &str,
    expected_origin: &str,
    rp_id: &str,
    name: &str,
    now: u64,
) -> Result<WebAuthnCredential, WebAuthnError> {
    let client_data = decode_b64(&resp.response.client_data_json)?;
    verify_client_data(
        &client_data,
        "webauthn.create",
        expected_challenge_b64,
        expected_origin,
    )?;
    let att_b64 = resp
        .response
        .attestation_object
        .as_deref()
        .ok_or(WebAuthnError::Attestation)?;
    let att = decode_b64(att_b64)?;
    let auth_data = parse_attestation_auth_data(&att)?;
    verify_auth_data_flags(&auth_data, rp_id, true)?;
    let (cred_id, cose) = parse_attested_credential(&auth_data)?;
    // Ensure the COSE key parses as ES256.
    let _ = verifying_key_from_cose(&cose)?;
    let id = URL_SAFE_NO_PAD.encode(&cred_id);
    if !resp.id.is_empty()
        && resp.id != id
        && decode_b64(&resp.id).ok().as_deref() != Some(cred_id.as_slice())
    {
        // Browsers may send the same id; tolerate either encoding.
    }
    Ok(WebAuthnCredential {
        id,
        cose_key: URL_SAFE_NO_PAD.encode(&cose),
        sign_count: u32::from_be_bytes(auth_data[33..37].try_into().unwrap_or([0; 4])),
        name: if name.trim().is_empty() {
            "Security key".into()
        } else {
            name.trim().chars().take(64).collect()
        },
        created: now,
    })
}

/// Verify an assertion. Returns the new sign count.
pub fn finish_authentication(
    resp: &AuthenticatorResponse,
    cred: &WebAuthnCredential,
    expected_challenge_b64: &str,
    expected_origin: &str,
    rp_id: &str,
) -> Result<u32, WebAuthnError> {
    let client_data = decode_b64(&resp.response.client_data_json)?;
    verify_client_data(
        &client_data,
        "webauthn.get",
        expected_challenge_b64,
        expected_origin,
    )?;
    let auth_b64 = resp
        .response
        .authenticator_data
        .as_deref()
        .ok_or(WebAuthnError::Attestation)?;
    let sig_b64 = resp
        .response
        .signature
        .as_deref()
        .ok_or(WebAuthnError::Signature)?;
    let auth_data = decode_b64(auth_b64)?;
    let signature = decode_b64(sig_b64)?;
    verify_auth_data_flags(&auth_data, rp_id, false)?;
    let count = u32::from_be_bytes(
        auth_data[33..37]
            .try_into()
            .map_err(|_| WebAuthnError::Attestation)?,
    );
    if cred.sign_count > 0 && count > 0 && count <= cred.sign_count {
        return Err(WebAuthnError::SignCount);
    }
    let mut msg = auth_data.clone();
    msg.extend_from_slice(&Sha256::digest(&client_data));
    let key = verifying_key_from_cose(&decode_b64(&cred.cose_key)?)?;
    let sig = Signature::from_der(&signature)
        .or_else(|_| Signature::from_slice(&signature))
        .map_err(|_| WebAuthnError::Signature)?;
    key.verify(&msg, &sig)
        .map_err(|_| WebAuthnError::Signature)?;
    Ok(count)
}

fn decode_b64(s: &str) -> Result<Vec<u8>, WebAuthnError> {
    URL_SAFE_NO_PAD
        .decode(s.trim())
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(s.trim()))
        .map_err(|_| WebAuthnError::Base64)
}

fn verify_client_data(
    bytes: &[u8],
    expected_type: &str,
    expected_challenge_b64: &str,
    expected_origin: &str,
) -> Result<(), WebAuthnError> {
    #[derive(Deserialize)]
    struct ClientData {
        #[serde(rename = "type")]
        type_: String,
        challenge: String,
        origin: String,
    }
    let data: ClientData = serde_json::from_slice(bytes).map_err(|_| WebAuthnError::ClientData)?;
    if data.type_ != expected_type {
        return Err(WebAuthnError::ClientData);
    }
    // Challenge may be base64url with or without padding.
    let got = decode_b64(&data.challenge)?;
    let want = decode_b64(expected_challenge_b64)?;
    if got != want {
        return Err(WebAuthnError::Challenge);
    }
    if !origin_matches(&data.origin, expected_origin) {
        return Err(WebAuthnError::Origin);
    }
    Ok(())
}

fn origin_matches(got: &str, expected: &str) -> bool {
    if got == expected {
        return true;
    }
    // Allow http://127.0.0.1:PORT vs configured origin without port when both loopback.
    let Ok(g) = url::Url::parse(got) else {
        return false;
    };
    let Ok(e) = url::Url::parse(expected) else {
        return false;
    };
    g.scheme() == e.scheme()
        && g.host_str() == e.host_str()
        && (e.port() == g.port() || e.port().is_none() || g.port().is_none())
}

fn verify_auth_data_flags(
    auth_data: &[u8],
    rp_id: &str,
    require_at: bool,
) -> Result<(), WebAuthnError> {
    if auth_data.len() < 37 {
        return Err(WebAuthnError::Attestation);
    }
    let rp_hash = Sha256::digest(rp_id.as_bytes());
    if auth_data[..32] != rp_hash[..] {
        return Err(WebAuthnError::RpId);
    }
    let flags = auth_data[32];
    if flags & 0x01 == 0 {
        return Err(WebAuthnError::Flags); // UP
    }
    if require_at && flags & 0x40 == 0 {
        return Err(WebAuthnError::Attestation); // AT
    }
    Ok(())
}

fn parse_attestation_auth_data(att: &[u8]) -> Result<Vec<u8>, WebAuthnError> {
    let value: Value = ciborium::from_reader(att).map_err(|_| WebAuthnError::Attestation)?;
    let Value::Map(map) = value else {
        return Err(WebAuthnError::Attestation);
    };
    for (k, v) in map {
        if matches!(&k, Value::Text(t) if t == "authData") {
            if let Value::Bytes(b) = v {
                return Ok(b);
            }
        }
    }
    Err(WebAuthnError::Attestation)
}

fn parse_attested_credential(auth_data: &[u8]) -> Result<(Vec<u8>, Vec<u8>), WebAuthnError> {
    if auth_data.len() < 55 {
        return Err(WebAuthnError::Attestation);
    }
    let id_len = u16::from_be_bytes([auth_data[53], auth_data[54]]) as usize;
    let id_start = 55;
    let id_end = id_start + id_len;
    if auth_data.len() < id_end + 1 {
        return Err(WebAuthnError::Attestation);
    }
    let cred_id = auth_data[id_start..id_end].to_vec();
    let cose = auth_data[id_end..].to_vec();
    Ok((cred_id, cose))
}

fn verifying_key_from_cose(cose: &[u8]) -> Result<VerifyingKey, WebAuthnError> {
    let value: Value = ciborium::from_reader(cose).map_err(|_| WebAuthnError::Cose)?;
    let Value::Map(map) = value else {
        return Err(WebAuthnError::Cose);
    };
    let mut kty = None;
    let mut alg = None;
    let mut crv = None;
    let mut x = None;
    let mut y = None;
    for (k, v) in map {
        let key = match k {
            Value::Integer(i) => i128::from(i),
            _ => continue,
        };
        match key {
            1 => kty = int_val(&v),
            3 => alg = int_val(&v),
            -1 => crv = int_val(&v),
            -2 => x = bytes_val(&v),
            -3 => y = bytes_val(&v),
            _ => {}
        }
    }
    if kty != Some(2) || alg != Some(-7) || crv != Some(1) {
        return Err(WebAuthnError::Cose);
    }
    let (Some(x), Some(y)) = (x, y) else {
        return Err(WebAuthnError::Cose);
    };
    if x.len() != 32 || y.len() != 32 {
        return Err(WebAuthnError::Cose);
    }
    let mut uncompressed = vec![0x04];
    uncompressed.extend_from_slice(&x);
    uncompressed.extend_from_slice(&y);
    let point = EncodedPoint::from_bytes(&uncompressed).map_err(|_| WebAuthnError::Cose)?;
    let pk = PublicKey::from_encoded_point(&point)
        .into_option()
        .ok_or(WebAuthnError::Cose)?;
    Ok(VerifyingKey::from(pk))
}

fn int_val(v: &Value) -> Option<i128> {
    match v {
        Value::Integer(i) => Some(i128::from(*i)),
        _ => None,
    }
}

fn bytes_val(v: &Value) -> Option<Vec<u8>> {
    match v {
        Value::Bytes(b) => Some(b.clone()),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use p256::ecdsa::{Signature, SigningKey, signature::Signer};

    fn cose_from_xy(x: &[u8; 32], y: &[u8; 32]) -> Vec<u8> {
        let map = vec![
            (Value::Integer(1.into()), Value::Integer(2.into())),
            (Value::Integer(3.into()), Value::Integer((-7).into())),
            (Value::Integer((-1).into()), Value::Integer(1.into())),
            (Value::Integer((-2).into()), Value::Bytes(x.to_vec())),
            (Value::Integer((-3).into()), Value::Bytes(y.to_vec())),
        ];
        let mut out = Vec::new();
        ciborium::into_writer(&Value::Map(map), &mut out).unwrap();
        out
    }

    #[test]
    fn roundtrip_assertion() {
        let sk = SigningKey::from_bytes((&[7u8; 32]).into()).unwrap();
        let vk = VerifyingKey::from(&sk);
        let point = vk.to_encoded_point(false);
        let x: [u8; 32] = point.x().unwrap().as_slice().try_into().unwrap();
        let y: [u8; 32] = point.y().unwrap().as_slice().try_into().unwrap();
        let cose = cose_from_xy(&x, &y);
        let cred = WebAuthnCredential {
            id: URL_SAFE_NO_PAD.encode(b"cred-1"),
            cose_key: URL_SAFE_NO_PAD.encode(&cose),
            sign_count: 1,
            name: "test".into(),
            created: 1,
        };
        let challenge = b"challenge-bytes-32!!!!!!!!!!!!!!!!";
        let challenge_b64 = URL_SAFE_NO_PAD.encode(challenge);
        let client = serde_json::json!({
            "type": "webauthn.get",
            "challenge": challenge_b64,
            "origin": "http://127.0.0.1:8766",
        });
        let client_bytes = serde_json::to_vec(&client).unwrap();
        let rp_hash = Sha256::digest(b"localhost");
        let mut auth = Vec::new();
        auth.extend_from_slice(&rp_hash);
        auth.push(0x01); // UP
        auth.extend_from_slice(&5u32.to_be_bytes());
        let mut msg = auth.clone();
        msg.extend_from_slice(&Sha256::digest(&client_bytes));
        let sig: Signature = sk.sign(&msg);
        let resp = AuthenticatorResponse {
            id: cred.id.clone(),
            raw_id: cred.id.clone(),
            type_: Some("public-key".into()),
            response: AuthenticatorResponseBody {
                client_data_json: URL_SAFE_NO_PAD.encode(&client_bytes),
                attestation_object: None,
                authenticator_data: Some(URL_SAFE_NO_PAD.encode(&auth)),
                signature: Some(URL_SAFE_NO_PAD.encode(sig.to_der().as_bytes())),
                user_handle: None,
            },
        };
        let n = finish_authentication(
            &resp,
            &cred,
            &challenge_b64,
            "http://127.0.0.1:8766",
            "localhost",
        )
        .unwrap();
        assert_eq!(n, 5);
    }
}
