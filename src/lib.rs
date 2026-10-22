mod bits;
mod clustering;
mod prescription;
mod symbol;
#[cfg(target_arch = "wasm32")]
mod wasm;

use prescription::{AxisPolicy, DiopterPolicy, EyeInput};
use serde::Deserialize;
use serde_json::{json, Value};

pub const MAX_REQUEST_BYTES: usize = 65_536;

#[derive(Debug)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
}
type Result<T> = std::result::Result<T, Error>;
impl Error {
    fn input(message: impl Into<String>) -> Self {
        Self {
            code: "BAD_INPUT",
            message: message.into(),
        }
    }
    fn symbol(message: impl Into<String>) -> Self {
        Self {
            code: "INVALID_SYMBOL",
            message: message.into(),
        }
    }
    fn topology(message: impl Into<String>) -> Self {
        Self {
            code: "TOPOLOGY",
            message: message.into(),
        }
    }
    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "INTERNAL",
            message: message.into(),
        }
    }
}

fn default_lens_type() -> u8 {
    3
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Config {},
    Review {
        #[serde(default)]
        left: Option<EyeInput>,
        #[serde(default)]
        right: Option<EyeInput>,
        #[serde(default)]
        axis_policy: AxisPolicy,
    },
    Generate {
        #[serde(default)]
        left: Option<EyeInput>,
        #[serde(default)]
        right: Option<EyeInput>,
        #[serde(default)]
        axis_policy: AxisPolicy,
        #[serde(default)]
        diopter_policy: DiopterPolicy,
        #[serde(default = "default_lens_type")]
        lens_type: u8,
        #[serde(default)]
        random_bits: u8,
        secret_hex: String,
    },
    EncodePayload {
        payload_hex: String,
    },
    DecodeSymbol {
        gaps: Vec<u8>,
        colors: Vec<u8>,
    },
}

fn execute(request: Request) -> Result<Value> {
    match request {
        Request::Config {} => Ok(prescription::config()),
        Request::Review {
            left,
            right,
            axis_policy,
        } => Ok(prescription::review(
            left.as_ref(),
            right.as_ref(),
            axis_policy,
        )),
        Request::EncodePayload { payload_hex } => {
            symbol::encode(&bits::parse_hex(&payload_hex, "payload_hex")?)
        }
        Request::DecodeSymbol { gaps, colors } => symbol::decode(&gaps, &colors),
        Request::Generate {
            left,
            right,
            axis_policy,
            diopter_policy,
            lens_type,
            random_bits,
            secret_hex,
        } => {
            let prepared = if diopter_policy == DiopterPolicy::Exact {
                prescription::prepare(
                    left.as_ref(),
                    right.as_ref(),
                    axis_policy,
                    lens_type,
                    random_bits,
                    &secret_hex,
                )?
            } else {
                prescription::prepare_with_policy(
                    left.as_ref(),
                    right.as_ref(),
                    axis_policy,
                    diopter_policy,
                    lens_type,
                    random_bits,
                    &secret_hex,
                )?
            };
            let mut result = symbol::encode(&prepared.payload)?;
            result["entered"] = prepared.entered;
            result["encoded"] = prepared.encoded;
            result["calibration"] = prepared.calibration;
            result["warnings"] = json!(prepared.warnings);
            result["profile_adjustments"] = prepared.profile_adjustments;
            result["diopter_policy"] = json!(diopter_policy);
            result["payload_version"] = json!(5);
            result["lens_type"] = json!(lens_type);
            result["random_bits"] = json!(random_bits);
            Ok(result)
        }
    }
}

pub fn error_response(code: &str, message: &str) -> String {
    json!({"ok":false,"error":{"code":code,"message":message}}).to_string()
}

/// Process one bounded UTF-8 JSON request. No filesystem, entropy, or network access occurs here.
pub fn process_json(input: &[u8]) -> String {
    if input.len() > MAX_REQUEST_BYTES {
        return error_response("BAD_INPUT", "Request exceeds 65536 bytes");
    }
    let request = match serde_json::from_slice::<Request>(input) {
        Ok(request) => request,
        Err(error) => {
            return error_response("BAD_INPUT", &format!("Invalid JSON request: {error}"))
        }
    };
    match execute(request) {
        Ok(result) => json!({"ok":true,"result":result}).to_string(),
        Err(error) => error_response(error.code, &error.message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn response(request: &Value) -> Value {
        serde_json::from_str(&process_json(request.to_string().as_bytes())).unwrap()
    }
    #[test]
    fn prescription_black_box_vectors() {
        let fixtures: Value =
            serde_json::from_str(include_str!("../fixtures/prescriptions.json")).unwrap();
        for vector in fixtures["vectors"].as_array().unwrap() {
            let result = response(&vector["request"]);
            assert_eq!(result["ok"], vector["ok"], "{result}");
            if vector["ok"] == true {
                assert_eq!(result["result"]["payload_hex"], vector["payload_hex"]);
                for side in ["left", "right"] {
                    if vector["request"][side].is_null() {
                        assert!(result["result"]["encoded"][side].is_null());
                        continue;
                    }
                    for field in [
                        "sphere",
                        "cylinder",
                        "axis",
                        "rx_id",
                        "axis_id",
                        "rx_offset_id",
                        "canonical",
                    ] {
                        assert_eq!(
                            result["result"]["encoded"][side][field],
                            vector["encoded"][side][field],
                            "{side} {field}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn all_supported_pairs_and_axis_entries_encode_without_approximation() {
        for entry in &prescription::tables().rx {
            for axis in &prescription::tables().axis {
                let eye = EyeInput {
                    sphere: entry.sphere_cd as f64 / 100.0,
                    cylinder: entry.cylinder_cd as f64 / 100.0,
                    axis: axis.degrees,
                    rx_offset_id: 0,
                };
                let prepared = prescription::prepare(
                    Some(&eye),
                    None,
                    AxisPolicy::Exact,
                    3,
                    0,
                    "00010203040506070809",
                )
                .unwrap();
                assert_eq!(prepared.encoded["left"]["rx_id"], entry.id);
                assert_eq!(prepared.encoded["left"]["axis_id"], axis.id);
            }
        }
    }
    #[test]
    fn malformed_and_oversized_requests_fail_without_panics() {
        for text in ["", "null", "[]", "{", "{\"op\":\"unknown\"}", "{\"op\":\"config\",\"width\":0}", "{\"op\":\"decode_symbol\",\"gaps\":[2],\"colors\":[]}", "{\"op\":\"encode_payload\",\"payload_hex\":\"00\"}", "{\"op\":\"generate\",\"left\":{\"sphere\":NaN,\"cylinder\":0,\"axis\":0},\"secret_hex\":\"00010203040506070809\"}"] {
            let v: Value=serde_json::from_str(&process_json(text.as_bytes())).unwrap(); assert_eq!(v["ok"],false);
        }
        let v: Value =
            serde_json::from_str(&process_json(&vec![b' '; MAX_REQUEST_BYTES + 1])).unwrap();
        assert_eq!(v["ok"], false);
    }
}
