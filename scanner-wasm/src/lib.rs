use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Serialize;
use spinwin_core::{verify_ticket, verifying_key_from_bytes};
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct WasmVerifyResult {
    valid: bool,
    ticket_id: Option<String>,
    name: Option<String>,
    email: Option<String>,
    prize: Option<String>,
    error: Option<String>,
}

#[wasm_bindgen]
pub fn verify_ticket_wasm(public_key_b64: &str, token: &str) -> JsValue {
    let result = (|| -> Result<WasmVerifyResult, String> {
        let key_bytes = URL_SAFE_NO_PAD
            .decode(public_key_b64)
            .map_err(|e| format!("Invalid public key base64: {}", e))?;

        let key_array: [u8; 32] = key_bytes
            .try_into()
            .map_err(|_| "Public key must be 32 bytes".to_string())?;

        let verifying_key =
            verifying_key_from_bytes(&key_array).map_err(|e| format!("Invalid public key: {}", e))?;

        let verify_result =
            verify_ticket(&verifying_key, token).map_err(|e| format!("Verification error: {}", e))?;

        if verify_result.valid {
            Ok(WasmVerifyResult {
                valid: true,
                ticket_id: Some(verify_result.payload.ticket_id),
                name: Some(verify_result.payload.name),
                email: Some(verify_result.payload.email),
                prize: Some(verify_result.payload.prize_name),
                error: None,
            })
        } else {
            Ok(WasmVerifyResult {
                valid: false,
                ticket_id: None,
                name: None,
                email: None,
                prize: None,
                error: Some("Signature verification failed".to_string()),
            })
        }
    })();

    let output = match result {
        Ok(r) => r,
        Err(e) => WasmVerifyResult {
            valid: false,
            ticket_id: None,
            name: None,
            email: None,
            prize: None,
            error: Some(e),
        },
    };

    serde_wasm_bindgen::to_value(&output).unwrap_or(JsValue::NULL)
}
