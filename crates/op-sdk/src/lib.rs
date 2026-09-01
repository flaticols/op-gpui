//! Unofficial Rust access to 1Password's desktop-app SDK integration.
//!
//! This crate mirrors the private protocol used by the official Go SDK. The
//! protocol is not a stable public 1Password API and may change without notice.

mod error;
mod protocol;
mod transport;
mod types;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[cfg(any(target_os = "macos", test))]
use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value};
use zeroize::Zeroizing;

pub use error::{Error, Result};
pub use types::{
    FieldId, FieldKind, FieldOverview, ItemCategory, ItemId, ItemOverview, SecretReference,
    SecretValue, SectionId, VaultId, VaultOverview,
};

use crate::{
    protocol::{
        ClientConfig, Invocation, InvokeConfig, Parameters, Request, decode_response,
        encode_request, protocol_error,
    },
    transport::Transport,
};

#[cfg(any(target_os = "macos", test))]
const DYLIB_RELATIVE_PATH: &str = "1Password.app/Contents/Frameworks/libop_sdk_ipc_client.dylib";

/// A cloneable authenticated client for the 1Password desktop application.
#[derive(Clone)]
pub struct Client {
    inner: Arc<ClientInner>,
}

struct ClientInner {
    transport: Arc<dyn Transport>,
    account_name: String,
    integration: IntegrationInfo,
    client_id: Mutex<Option<u64>>,
    operation_lock: Mutex<()>,
}

#[derive(Clone, Debug)]
struct IntegrationInfo {
    name: String,
    version: String,
}

/// Builds a desktop-authenticated [`Client`].
#[derive(Clone, Debug, Default)]
pub struct ClientBuilder {
    account_name: Option<String>,
    integration: Option<IntegrationInfo>,
    library_path: Option<PathBuf>,
}

impl Client {
    /// Starts configuring a desktop-authenticated client.
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Lists the vaults available to the authenticated desktop session.
    pub fn vaults(&self) -> Result<Vec<VaultOverview>> {
        let mut parameters = Map::new();
        parameters.insert("params".to_owned(), Value::Null);
        self.invoke_typed("VaultsList", parameters)
    }

    /// Lists active item overviews in a vault.
    pub fn items(&self, vault_id: &VaultId) -> Result<Vec<ItemOverview>> {
        let mut parameters = Map::new();
        parameters.insert("vault_id".to_owned(), Value::String(vault_id.to_string()));
        parameters.insert("filters".to_owned(), Value::Array(Vec::new()));
        self.invoke_typed("ItemsList", parameters)
    }

    /// Returns field metadata for an item without retaining field values.
    pub fn fields(&self, vault_id: &VaultId, item_id: &ItemId) -> Result<Vec<FieldOverview>> {
        let mut parameters = Map::new();
        parameters.insert("vault_id".to_owned(), Value::String(vault_id.to_string()));
        parameters.insert("item_id".to_owned(), Value::String(item_id.to_string()));
        let item: WireItem = self.invoke_typed("ItemsGet", parameters)?;
        let sections = item
            .sections
            .into_iter()
            .map(|section| (section.id, section.title))
            .collect::<HashMap<_, _>>();
        item.fields
            .into_iter()
            .map(|field| {
                let section_title = field
                    .section_id
                    .as_ref()
                    .and_then(|section_id| sections.get(section_id))
                    .cloned();
                Ok(FieldOverview::new(
                    field.id,
                    field.title,
                    field.section_id,
                    section_title,
                    field.kind,
                ))
            })
            .collect()
    }

    /// Builds an `op://` reference using stable vault, item, section, and field IDs.
    pub fn secret_reference(
        &self,
        vault_id: &VaultId,
        item_id: &ItemId,
        field: &FieldOverview,
    ) -> Result<SecretReference> {
        SecretReference::for_field(vault_id, item_id, field)
    }

    /// Resolves an `op://` reference through the authenticated desktop session.
    pub fn resolve(&self, reference: &SecretReference) -> Result<SecretValue> {
        let mut parameters = Map::new();
        parameters.insert(
            "secret_reference".to_owned(),
            Value::String(reference.to_string()),
        );
        let value: String = self.invoke_typed("SecretsResolve", parameters)?;
        Ok(SecretValue::new(value))
    }

    /// Releases the desktop SDK client. Calling this more than once is safe.
    pub fn close(&self) -> Result<()> {
        let _operation = self
            .inner
            .operation_lock
            .lock()
            .map_err(|_| Error::LockPoisoned)?;
        self.inner.release_locked()
    }

    #[cfg(feature = "unstable-raw")]
    /// Invokes an unmodeled private-protocol method.
    ///
    /// This escape hatch has no compatibility guarantee and may return values
    /// that are not cleared from memory by `serde_json::Value`.
    pub fn raw_invoke(&self, method: &str, parameters: Map<String, Value>) -> Result<Value> {
        self.invoke_typed(method, parameters)
    }

    fn invoke_typed<T>(&self, method: &str, parameters: Map<String, Value>) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let response = self.invoke_bytes(method, parameters)?;
        let response = Zeroizing::new(response);
        serde_json::from_slice(response.as_slice()).map_err(protocol_error)
    }

    fn invoke_bytes(&self, method: &str, parameters: Map<String, Value>) -> Result<Vec<u8>> {
        let _operation = self
            .inner
            .operation_lock
            .lock()
            .map_err(|_| Error::LockPoisoned)?;
        let client_id = self.inner.current_client_id()?;
        match self
            .inner
            .invoke_once(client_id, method, parameters.clone())
        {
            Err(error) if error.is_session_expired() => {
                let renewed = self.inner.initialize()?;
                *self
                    .inner
                    .client_id
                    .lock()
                    .map_err(|_| Error::LockPoisoned)? = Some(renewed);
                self.inner.invoke_once(renewed, method, parameters)
            }
            result => result,
        }
    }
}

impl ClientBuilder {
    /// Selects desktop-app authentication for an account name or account UUID.
    pub fn desktop(mut self, account_name: impl Into<String>) -> Self {
        self.account_name = Some(account_name.into());
        self
    }

    /// Sets the integration identity shown to 1Password.
    pub fn integration(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.integration = Some(IntegrationInfo {
            name: name.into(),
            version: version.into(),
        });
        self
    }

    /// Overrides automatic discovery of the installed desktop dylib.
    pub fn library_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.library_path = Some(path.into());
        self
    }

    /// Loads the desktop dylib and initializes the authenticated client.
    pub fn connect(self) -> Result<Client> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = self;
            return Err(Error::UnsupportedPlatform);
        }

        #[cfg(target_os = "macos")]
        {
            let path = match self.library_path.as_deref() {
                Some(path) => path.to_path_buf(),
                None => find_library_path()?,
            };
            let path = std::fs::canonicalize(&path).unwrap_or(path);
            let transport = transport::load(&path)?;
            self.connect_with_transport(transport)
        }
    }

    #[cfg(any(target_os = "macos", test))]
    fn connect_with_transport(self, transport: Arc<dyn Transport>) -> Result<Client> {
        let account_name = required(self.account_name, "account name")?;
        let integration = self.integration.ok_or(Error::MissingConfiguration {
            field: "integration name and version",
        })?;
        if integration.name.trim().is_empty() {
            return Err(Error::MissingConfiguration {
                field: "integration name",
            });
        }
        if integration.version.trim().is_empty() {
            return Err(Error::MissingConfiguration {
                field: "integration version",
            });
        }
        let inner = Arc::new(ClientInner {
            transport,
            account_name,
            integration,
            client_id: Mutex::new(None),
            operation_lock: Mutex::new(()),
        });
        let client_id = inner.initialize()?;
        *inner.client_id.lock().map_err(|_| Error::LockPoisoned)? = Some(client_id);
        Ok(Client { inner })
    }
}

impl ClientInner {
    fn initialize(&self) -> Result<u64> {
        let packed = packed_version();
        let config = ClientConfig {
            service_account_token: "",
            programming_language: "Rust",
            sdk_version: &packed,
            integration_name: &self.integration.name,
            integration_version: &self.integration.version,
            request_library_name: "libloading",
            request_library_version: "0.8",
            os: wire_os(),
            os_version: "0.0.0",
            architecture: wire_architecture(),
            account_name: &self.account_name,
        };
        let payload = serde_json::to_vec(&config).map_err(protocol_error)?;
        let response = self.send("init_client", &payload)?;
        serde_json::from_slice(&response).map_err(protocol_error)
    }

    fn invoke_once(
        &self,
        client_id: u64,
        method: &str,
        parameters: Map<String, Value>,
    ) -> Result<Vec<u8>> {
        let invoke = InvokeConfig {
            invocation: Invocation {
                client_id,
                parameters: Parameters {
                    name: method,
                    parameters,
                },
            },
        };
        let payload = serde_json::to_vec(&invoke).map_err(protocol_error)?;
        self.send("invoke", &payload)
    }

    fn send(&self, kind: &str, payload: &[u8]) -> Result<Vec<u8>> {
        let request = encode_request(&Request {
            kind,
            account_name: &self.account_name,
            payload,
        })?;
        let response = Zeroizing::new(self.transport.call(&request)?);
        decode_response(response.as_slice())
    }

    fn current_client_id(&self) -> Result<u64> {
        self.client_id
            .lock()
            .map_err(|_| Error::LockPoisoned)?
            .ok_or(Error::ClientClosed)
    }

    fn release_locked(&self) -> Result<()> {
        let Some(client_id) = self
            .client_id
            .lock()
            .map_err(|_| Error::LockPoisoned)?
            .take()
        else {
            return Ok(());
        };
        let payload = serde_json::to_vec(&client_id).map_err(protocol_error)?;
        self.send("release_client", &payload).map(|_| ())
    }
}

impl Drop for ClientInner {
    fn drop(&mut self) {
        let Ok(client_id) = self.client_id.get_mut() else {
            return;
        };
        let Some(client_id) = client_id.take() else {
            return;
        };
        let Ok(payload) = serde_json::to_vec(&client_id) else {
            return;
        };
        let _ = self.send("release_client", &payload);
    }
}

#[derive(Deserialize)]
struct WireItem {
    #[serde(default)]
    fields: Vec<WireField>,
    #[serde(default)]
    sections: Vec<WireSection>,
}

#[derive(Deserialize)]
struct WireField {
    id: FieldId,
    title: String,
    #[serde(default, rename = "sectionId")]
    section_id: Option<SectionId>,
    #[serde(rename = "fieldType")]
    kind: FieldKind,
}

#[derive(Deserialize)]
struct WireSection {
    id: SectionId,
    title: String,
}

#[cfg(any(target_os = "macos", test))]
fn required(value: Option<String>, field: &'static str) -> Result<String> {
    match value {
        Some(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(Error::MissingConfiguration { field }),
    }
}

#[cfg(target_os = "macos")]
fn find_library_path() -> Result<PathBuf> {
    let candidates = library_candidates(std::env::var_os("HOME").as_deref());
    find_library_in(candidates, Path::is_file)
}

#[cfg(any(target_os = "macos", test))]
fn library_candidates(home: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("/Applications").join(DYLIB_RELATIVE_PATH)];
    if let Some(home) = home {
        candidates.push(
            Path::new(home)
                .join("Applications")
                .join(DYLIB_RELATIVE_PATH),
        );
    }
    candidates
}

#[cfg(any(target_os = "macos", test))]
fn find_library_in(candidates: Vec<PathBuf>, exists: impl Fn(&Path) -> bool) -> Result<PathBuf> {
    candidates
        .iter()
        .find(|candidate| exists(candidate))
        .cloned()
        .ok_or(Error::ApplicationNotFound {
            searched_paths: candidates,
        })
}

/// This crate's own version in the packed form the desktop app requires.
///
/// **Not a semver, and that is the whole point.** The relay validates this
/// field's *shape*: a `"0.1.0"` is refused, and the refusal arrives as
/// `Failed to delegate a session ... HttpStatus(400)` from 1Password's server -
/// naming neither this field nor a format, and looking for all the world like
/// an unlock or entitlement problem. It is not; every account and every other
/// field was eliminated before this one was found by diffing against the
/// official Go SDK on the wire.
///
/// The encoding is `major(1) minor(2) patch(2) build(2)`, read off that SDK's
/// own embedded `release/version-build`: `0.4.0` build 3 is `0040003`, `0.4.1`
/// build 2 is `0040102`. There is no minimum - `0010000` and `9990000` are both
/// accepted - so this reports the truth about itself rather than borrowing a
/// version it does not have. Build is always 0: this crate has no fourth
/// component to report.
fn packed_version() -> String {
    let major: u32 = env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap_or(0);
    let minor: u32 = env!("CARGO_PKG_VERSION_MINOR").parse().unwrap_or(0);
    let patch: u32 = env!("CARGO_PKG_VERSION_PATCH").parse().unwrap_or(0);
    format!("{:01}{:02}{:02}{:02}", major % 10, minor % 100, patch % 100, 0)
}

const fn wire_os() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        std::env::consts::OS
    }
}

const fn wire_architecture() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "amd64"
    } else {
        std::env::consts::ARCH
    }
}

#[cfg(test)]
mod tests;
