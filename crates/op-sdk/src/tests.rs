use std::sync::{Arc, Mutex};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

use super::*;

#[derive(Default)]
struct FakeTransport {
    requests: Mutex<Vec<Value>>,
    responses: Mutex<Vec<Result<Vec<u8>>>>,
}

impl FakeTransport {
    fn with_responses(responses: impl IntoIterator<Item = Result<Vec<u8>>>) -> Arc<Self> {
        let mut responses = responses.into_iter().collect::<Vec<_>>();
        responses.reverse();
        Arc::new(Self {
            requests: Mutex::new(Vec::new()),
            responses: Mutex::new(responses),
        })
    }

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }
}

impl Transport for FakeTransport {
    fn call(&self, input: &[u8]) -> Result<Vec<u8>> {
        self.requests
            .lock()
            .unwrap()
            .push(serde_json::from_slice(input).unwrap());
        self.responses
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| success(Value::Null))
    }
}

fn success(payload: Value) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(&payload).unwrap();
    Ok(serde_json::to_vec(&json!({
        "success": true,
        "payload": STANDARD.encode(payload),
    }))
    .unwrap())
}

fn success_bytes(payload: Value) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(&payload).unwrap();
    Ok(serde_json::to_vec(&json!({
        "success": true,
        "payload": payload,
    }))
    .unwrap())
}

fn remote_error(name: &str, message: &str) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(&json!({ "name": name, "message": message })).unwrap();
    Ok(serde_json::to_vec(&json!({
        "success": false,
        "payload": STANDARD.encode(payload),
    }))
    .unwrap())
}

fn remote_error_bytes(name: &str, message: &str) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(&json!({ "name": name, "message": message })).unwrap();
    Ok(serde_json::to_vec(&json!({
        "success": false,
        "payload": payload,
    }))
    .unwrap())
}

fn client_with(transport: Arc<dyn Transport>) -> Result<Client> {
    Client::builder()
        .desktop("Example")
        .integration("Test integration", "1.0.0")
        .connect_with_transport(transport)
}

#[test]
fn request_payloads_match_go_base64_encoding() {
    let transport = FakeTransport::with_responses([success(json!(42))]);
    let client = client_with(transport.clone()).unwrap();
    let requests = transport.requests();
    let request = &requests[0];
    assert_eq!(request["kind"], "init_client");
    assert_eq!(request["account_name"], "Example");
    let payload = STANDARD
        .decode(request["payload"].as_str().unwrap())
        .unwrap();
    let config: Value = serde_json::from_slice(&payload).unwrap();
    assert_eq!(config["programmingLanguage"], "Rust");
    assert_eq!(config["account_name"], "Example");
    assert_eq!(config["architecture"], wire_architecture());
    drop(client);
}

#[test]
fn response_payloads_accept_base64_strings_and_byte_arrays() {
    let expected = serde_json::to_vec(&json!({ "client_id": 42 })).unwrap();

    assert_eq!(
        decode_response(&success(json!({ "client_id": 42 })).unwrap()).unwrap(),
        expected
    );
    assert_eq!(
        decode_response(&success_bytes(json!({ "client_id": 42 })).unwrap()).unwrap(),
        expected
    );
}

#[test]
fn byte_array_response_envelopes_work_through_the_client() {
    let transport = FakeTransport::with_responses([
        success_bytes(json!(7)),
        success_bytes(json!([{
            "id": "v1", "title": "Private", "description": "", "activeItemCount": 1
        }])),
    ]);

    let client = client_with(transport).unwrap();
    let vaults = client.vaults().unwrap();

    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0].title(), "Private");
}

#[test]
fn byte_array_remote_errors_remain_structured() {
    let response = remote_error_bytes("DesktopError", "not available").unwrap();

    assert!(matches!(
        decode_response(&response),
        Err(Error::Remote { name, message })
            if name == "DesktopError" && message == "not available"
    ));
}

#[test]
fn typed_browsing_and_secret_resolution() {
    let transport = FakeTransport::with_responses([
        success(json!(7)),
        success(json!([{
            "id": "v1", "title": "Private", "description": "", "activeItemCount": 1
        }])),
        success(json!([{
            "id": "i1", "title": "Example", "category": "Login", "vaultId": "v1"
        }])),
        success(json!({
            "fields": [
                {"id": "username", "title": "username", "fieldType": "Text", "value": "alice"},
                {"id": "custom", "title": "API key", "sectionId": "s1", "fieldType": "Concealed", "value": "secret"}
            ],
            "sections": [{"id": "s1", "title": "Tokens"}]
        })),
        success(json!("resolved secret")),
    ]);
    let client = client_with(transport).unwrap();
    let vaults = client.vaults().unwrap();
    let items = client.items(vaults[0].id()).unwrap();
    let fields = client.fields(vaults[0].id(), items[0].id()).unwrap();
    assert_eq!(fields[1].section_title(), Some("Tokens"));
    let reference = client
        .secret_reference(vaults[0].id(), items[0].id(), &fields[1])
        .unwrap();
    assert_eq!(reference.as_str(), "op://v1/i1/s1/custom");
    let secret = client.resolve(&reference).unwrap();
    assert_eq!(secret.expose_secret(), "resolved secret");
    assert_eq!(format!("{secret:?}"), "SecretValue([REDACTED])");
    assert_eq!(secret.to_string(), "[REDACTED]");
}

#[test]
fn session_expiration_reinitializes_and_retries_once() {
    let transport = FakeTransport::with_responses([
        success(json!(1)),
        remote_error("DesktopSessionExpired", "expired"),
        success(json!(2)),
        success(json!([])),
    ]);
    let client = client_with(transport.clone()).unwrap();
    assert!(client.vaults().unwrap().is_empty());
    let kinds = transport
        .requests()
        .into_iter()
        .map(|request| request["kind"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["init_client", "invoke", "init_client", "invoke"]);
}

#[test]
fn close_is_idempotent_and_blocks_future_calls() {
    let transport = FakeTransport::with_responses([success(json!(1)), success(Value::Null)]);
    let client = client_with(transport.clone()).unwrap();
    client.close().unwrap();
    client.close().unwrap();
    assert!(matches!(client.vaults(), Err(Error::ClientClosed)));
    assert_eq!(transport.requests().len(), 2);
}

#[test]
fn client_builder_requires_identity() {
    let transport = FakeTransport::with_responses([]);
    let error = Client::builder()
        .integration("App", "1")
        .connect_with_transport(transport)
        .err()
        .unwrap();
    assert!(matches!(
        error,
        Error::MissingConfiguration {
            field: "account name"
        }
    ));
}

#[test]
fn references_reject_ambiguous_segments() {
    assert!(SecretReference::parse("op://vault/item/field").is_ok());
    assert!(SecretReference::parse("op://vault/item/field?attribute=otp").is_ok());
    assert!(SecretReference::parse("https://vault/item/field").is_err());
    assert!(SecretReference::parse("op://vault/item").is_err());
    assert!(SecretReference::parse("op://vault/item/field?").is_err());
    assert!(SecretReference::parse("op://vault/item/field#fragment").is_err());
    assert!(VaultId::new("has/slash").is_err());
}

#[test]
fn unknown_remote_errors_remain_structured() {
    let transport = FakeTransport::with_responses([
        success(json!(1)),
        remote_error("NewServerError", "new behavior"),
    ]);
    let client = client_with(transport).unwrap();
    assert!(matches!(
        client.vaults(),
        Err(Error::Remote { name, message })
            if name == "NewServerError" && message == "new behavior"
    ));
}

#[test]
fn dylib_discovery_prefers_system_applications() {
    let candidates = library_candidates(Some(std::ffi::OsStr::new("/Users/example")));
    assert_eq!(candidates.len(), 2);
    assert_eq!(
        find_library_in(candidates.clone(), |_| true).unwrap(),
        candidates[0]
    );
    assert_eq!(
        find_library_in(candidates.clone(), |path| path == candidates[1]).unwrap(),
        candidates[1]
    );
    let error = find_library_in(candidates.clone(), |_| false).unwrap_err();
    assert!(matches!(
        error,
        Error::ApplicationNotFound { searched_paths } if searched_paths == candidates
    ));
}

#[test]
fn client_accounts_stay_isolated_on_a_shared_transport() {
    let transport = FakeTransport::with_responses([
        success(json!(1)),
        success(json!(2)),
        success(json!([])),
        success(json!([])),
    ]);
    let first = Client::builder()
        .desktop("First")
        .integration("App", "1")
        .connect_with_transport(transport.clone())
        .unwrap();
    let second = Client::builder()
        .desktop("Second")
        .integration("App", "1")
        .connect_with_transport(transport.clone())
        .unwrap();
    first.vaults().unwrap();
    second.vaults().unwrap();
    let accounts = transport
        .requests()
        .into_iter()
        .map(|request| request["account_name"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(accounts, ["First", "Second", "First", "Second"]);
}

#[test]
fn session_expiration_is_only_retried_once() {
    let transport = FakeTransport::with_responses([
        success(json!(1)),
        remote_error("DesktopSessionExpired", "expired"),
        success(json!(2)),
        remote_error("DesktopSessionExpired", "still expired"),
    ]);
    let client = client_with(transport.clone()).unwrap();
    assert!(matches!(
        client.vaults(),
        Err(Error::Remote { name, message })
            if name == "DesktopSessionExpired" && message == "still expired"
    ));
    assert_eq!(transport.requests().len(), 4);
}

#[test]
fn public_handles_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Client>();
    assert_send_sync::<SecretValue>();
}

#[test]
fn the_sdk_version_is_packed_rather_than_a_semver() {
    /* The desktop relay validates this field's SHAPE. A `"0.1.0"` is refused
    as `Failed to delegate a session ... HttpStatus(400)` from 1Password's
    server, which names neither this field nor a format - so the encoding is
    pinned here rather than left to be rediscovered by eliminating every
    account, every other field and the unlock path first.

    `major(1) minor(2) patch(2) build(2)`, read off the official Go SDK's own
    embedded `release/version-build`: 0.4.0 build 3 is `0040003`, 0.4.1 build 2
    is `0040102`. */
    let packed = super::packed_version();

    assert_eq!(packed.len(), 7, "{packed}");
    assert!(packed.chars().all(|c| c.is_ascii_digit()), "{packed}");
    assert!(!packed.contains('.'), "{packed}");
}

