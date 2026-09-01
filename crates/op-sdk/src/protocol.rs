use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use serde_json::{Map, Value};

use crate::{Error, Result};

#[derive(Debug, Serialize)]
pub(crate) struct Request<'a> {
    pub kind: &'a str,
    pub account_name: &'a str,
    #[serde(with = "base64_bytes")]
    pub payload: &'a [u8],
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Response {
    pub success: bool,
    #[serde(with = "base64_bytes")]
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClientConfig<'a> {
    pub service_account_token: &'a str,
    pub programming_language: &'static str,
    pub sdk_version: &'a str,
    pub integration_name: &'a str,
    pub integration_version: &'a str,
    pub request_library_name: &'static str,
    pub request_library_version: &'static str,
    pub os: &'static str,
    pub os_version: &'static str,
    pub architecture: &'static str,
    #[serde(rename = "account_name")]
    pub account_name: &'a str,
}

#[derive(Debug, Serialize)]
pub(crate) struct InvokeConfig<'a> {
    pub invocation: Invocation<'a>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Invocation<'a> {
    pub client_id: u64,
    pub parameters: Parameters<'a>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Parameters<'a> {
    pub name: &'a str,
    pub parameters: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
struct RemoteError {
    #[serde(default)]
    name: String,
    #[serde(default)]
    message: String,
}

pub(crate) fn encode_request(request: &Request<'_>) -> Result<Vec<u8>> {
    serde_json::to_vec(request).map_err(protocol_error)
}

pub(crate) fn decode_response(bytes: &[u8]) -> Result<Vec<u8>> {
    let response: Response = serde_json::from_slice(bytes).map_err(protocol_error)?;
    if response.success {
        return Ok(response.payload);
    }

    let remote: RemoteError =
        serde_json::from_slice(&response.payload).unwrap_or_else(|_| RemoteError {
            name: "Unknown".to_owned(),
            message: String::from_utf8_lossy(&response.payload).into_owned(),
        });
    Err(Error::Remote {
        name: if remote.name.is_empty() {
            "Unknown".to_owned()
        } else {
            remote.name
        },
        message: if remote.message.is_empty() {
            "the desktop app returned an unspecified error".to_owned()
        } else {
            remote.message
        },
    })
}

pub(crate) fn protocol_error(error: serde_json::Error) -> Error {
    Error::Protocol(error.to_string())
}

mod base64_bytes {
    use super::*;

    // Requests follow the Go SDK and encode bytes as base64. Desktop app
    // versions in the wild have returned either that string form or a JSON
    // byte array, so response decoding intentionally accepts both shapes.
    pub fn serialize<S>(bytes: &[u8], serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Payload {
            Base64(String),
            Bytes(Vec<u8>),
        }

        match Payload::deserialize(deserializer)? {
            Payload::Base64(encoded) => STANDARD.decode(encoded).map_err(D::Error::custom),
            Payload::Bytes(bytes) => Ok(bytes),
        }
    }
}
