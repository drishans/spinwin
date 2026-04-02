use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketPayload {
    pub ticket_id: String,
    pub email: String,
    pub name: String,
    pub prize_name: String,
    pub prize_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketToken {
    pub payload: TicketPayload,
    pub signature: String,
}

pub fn sign_ticket(signing_key: &SigningKey, payload: &TicketPayload) -> String {
    let payload_json = serde_json::to_string(payload).expect("serialize payload");
    let signature = signing_key.sign(payload_json.as_bytes());
    let token = TicketToken {
        payload: payload.clone(),
        signature: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
    };
    let token_json = serde_json::to_string(&token).expect("serialize token");
    URL_SAFE_NO_PAD.encode(token_json.as_bytes())
}

#[derive(Debug)]
pub enum VerifyError {
    InvalidBase64,
    InvalidJson,
    InvalidSignature,
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::InvalidBase64 => write!(f, "Invalid base64 encoding"),
            VerifyError::InvalidJson => write!(f, "Invalid JSON in token"),
            VerifyError::InvalidSignature => write!(f, "Invalid cryptographic signature"),
        }
    }
}

pub struct VerifyResult {
    pub payload: TicketPayload,
    pub valid: bool,
}

pub fn verify_ticket(verifying_key: &VerifyingKey, token: &str) -> Result<VerifyResult, VerifyError> {
    let token_bytes = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| VerifyError::InvalidBase64)?;
    let ticket_token: TicketToken =
        serde_json::from_slice(&token_bytes).map_err(|_| VerifyError::InvalidJson)?;

    let payload_json =
        serde_json::to_string(&ticket_token.payload).map_err(|_| VerifyError::InvalidJson)?;
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(&ticket_token.signature)
        .map_err(|_| VerifyError::InvalidBase64)?;
    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| VerifyError::InvalidSignature)?;
    let signature = Signature::from_bytes(&sig_array);

    let valid = verifying_key
        .verify(payload_json.as_bytes(), &signature)
        .is_ok();

    Ok(VerifyResult {
        payload: ticket_token.payload,
        valid,
    })
}

pub fn keypair_from_seed(seed: &[u8; 32]) -> (SigningKey, VerifyingKey) {
    let signing_key = SigningKey::from_bytes(seed);
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}

pub fn verifying_key_to_bytes(key: &VerifyingKey) -> [u8; 32] {
    key.to_bytes()
}

pub fn verifying_key_from_bytes(bytes: &[u8; 32]) -> Result<VerifyingKey, VerifyError> {
    VerifyingKey::from_bytes(bytes).map_err(|_| VerifyError::InvalidSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_verify() {
        let seed = [42u8; 32];
        let (signing_key, verifying_key) = keypair_from_seed(&seed);

        let payload = TicketPayload {
            ticket_id: "test-123".to_string(),
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            prize_name: "Necklace".to_string(),
            prize_id: 1,
        };

        let token = sign_ticket(&signing_key, &payload);
        let result = verify_ticket(&verifying_key, &token).unwrap();

        assert!(result.valid);
        assert_eq!(result.payload.ticket_id, "test-123");
        assert_eq!(result.payload.email, "test@example.com");
    }

    #[test]
    fn test_tampered_token_fails() {
        let seed = [42u8; 32];
        let (signing_key, verifying_key) = keypair_from_seed(&seed);

        let payload = TicketPayload {
            ticket_id: "test-123".to_string(),
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            prize_name: "Necklace".to_string(),
            prize_id: 1,
        };

        let token = sign_ticket(&signing_key, &payload);

        // Tamper with the token
        let mut token_bytes = URL_SAFE_NO_PAD.decode(&token).unwrap();
        if let Some(byte) = token_bytes.last_mut() {
            *byte ^= 0xFF;
        }
        let tampered = URL_SAFE_NO_PAD.encode(&token_bytes);

        // Should either fail to parse or have invalid signature
        match verify_ticket(&verifying_key, &tampered) {
            Ok(result) => assert!(!result.valid),
            Err(_) => {} // Also acceptable
        }
    }

    #[test]
    fn test_wrong_key_fails() {
        let seed1 = [42u8; 32];
        let seed2 = [99u8; 32];
        let (signing_key, _) = keypair_from_seed(&seed1);
        let (_, wrong_verifying_key) = keypair_from_seed(&seed2);

        let payload = TicketPayload {
            ticket_id: "test-123".to_string(),
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            prize_name: "Necklace".to_string(),
            prize_id: 1,
        };

        let token = sign_ticket(&signing_key, &payload);
        let result = verify_ticket(&wrong_verifying_key, &token).unwrap();
        assert!(!result.valid);
    }
}
