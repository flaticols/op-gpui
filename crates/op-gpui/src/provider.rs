use std::{error::Error, fmt, sync::Arc};

use op_sdk::{Client, SecretReference};

/// A cloneable, display-safe failure returned by a [`SecretProvider`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderError {
    message: Arc<str>,
}

impl ProviderError {
    /// Creates an error whose message can be shown in the picker.
    pub fn new(message: impl Into<Arc<str>>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the user-facing failure message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ProviderError {}

impl From<op_sdk::Error> for ProviderError {
    fn from(error: op_sdk::Error) -> Self {
        Self::new(error.to_string())
    }
}

/// Result returned by a [`SecretProvider`].
pub type ProviderResult<T> = Result<T, ProviderError>;

/// A vault row presented by [`crate::SecretPicker`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Vault {
    id: String,
    title: String,
    description: String,
    active_item_count: Option<u32>,
}

impl Vault {
    /// Creates a vault row with a stable provider-specific identifier.
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: String::new(),
            active_item_count: None,
        }
    }

    /// Adds supporting text shown below the vault title.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Adds the number of active items when the provider knows it.
    pub fn with_active_item_count(mut self, count: u32) -> Self {
        self.active_item_count = Some(count);
        self
    }

    /// Returns the stable provider-specific identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the display title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns optional supporting text.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Returns the known active-item count.
    pub fn active_item_count(&self) -> Option<u32> {
        self.active_item_count
    }
}

/// An item row presented by [`crate::SecretPicker`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Item {
    id: String,
    vault_id: String,
    title: String,
    category: String,
}

impl Item {
    /// Creates an item row with stable provider-specific identifiers.
    pub fn new(
        vault_id: impl Into<String>,
        id: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            vault_id: vault_id.into(),
            title: title.into(),
            category: String::new(),
        }
    }

    /// Adds the item category shown as supporting text.
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    /// Returns the stable item identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the stable containing-vault identifier.
    pub fn vault_id(&self) -> &str {
        &self.vault_id
    }

    /// Returns the display title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the provider's category label.
    pub fn category(&self) -> &str {
        &self.category
    }
}

/// A field row carrying the stable `op://` reference selected by the picker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    id: String,
    title: String,
    section_title: Option<String>,
    kind: String,
    reference: SecretReference,
}

impl Field {
    /// Creates a field row.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        reference: SecretReference,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            section_title: None,
            kind: String::new(),
            reference,
        }
    }

    /// Adds the containing section's display title.
    pub fn with_section_title(mut self, title: impl Into<String>) -> Self {
        self.section_title = Some(title.into());
        self
    }

    /// Adds the provider's field-kind label.
    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = kind.into();
        self
    }

    /// Returns the stable field identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the display title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the containing section's display title.
    pub fn section_title(&self) -> Option<&str> {
        self.section_title.as_deref()
    }

    /// Returns the provider's field-kind label.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Returns the stable reference represented by this row.
    pub fn reference(&self) -> &SecretReference {
        &self.reference
    }
}

/// Supplies the picker with vault, item, and field metadata.
///
/// Calls are synchronous because [`crate::SecretPicker`] always moves them to
/// GPUI's background executor. Implementations must not return plaintext field
/// values: field selection reports an `op://` reference for the application to
/// resolve only at its final use site.
pub trait SecretProvider: Send + Sync + 'static {
    /// Lists vaults visible to the current account.
    fn vaults(&self) -> ProviderResult<Vec<Vault>>;

    /// Lists items inside `vault_id`.
    fn items(&self, vault_id: &str) -> ProviderResult<Vec<Item>>;

    /// Lists selectable fields and their stable references.
    fn fields(&self, vault_id: &str, item_id: &str) -> ProviderResult<Vec<Field>>;
}

/// A [`SecretProvider`] backed by an authenticated [`op_sdk::Client`].
#[derive(Clone)]
pub struct OnePasswordProvider {
    client: Client,
}

impl OnePasswordProvider {
    /// Wraps an authenticated desktop SDK client.
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Returns the wrapped SDK client.
    pub fn client(&self) -> &Client {
        &self.client
    }
}

impl SecretProvider for OnePasswordProvider {
    fn vaults(&self) -> ProviderResult<Vec<Vault>> {
        self.client
            .vaults()?
            .into_iter()
            .map(|vault| {
                Ok(Vault::new(vault.id().to_string(), vault.title())
                    .with_description(vault.description())
                    .with_active_item_count(vault.active_item_count()))
            })
            .collect()
    }

    fn items(&self, vault_id: &str) -> ProviderResult<Vec<Item>> {
        let vault_id = op_sdk::VaultId::new(vault_id)?;
        self.client
            .items(&vault_id)?
            .into_iter()
            .map(|item| {
                Ok(Item::new(
                    item.vault_id().to_string(),
                    item.id().to_string(),
                    item.title(),
                )
                .with_category(item.category().to_string()))
            })
            .collect()
    }

    fn fields(&self, vault_id: &str, item_id: &str) -> ProviderResult<Vec<Field>> {
        let vault_id = op_sdk::VaultId::new(vault_id)?;
        let item_id = op_sdk::ItemId::new(item_id)?;
        self.client
            .fields(&vault_id, &item_id)?
            .into_iter()
            .map(|field| {
                let reference = self.client.secret_reference(&vault_id, &item_id, &field)?;
                let mut row = Field::new(field.id().to_string(), field.title(), reference)
                    .with_kind(field.kind().to_string());
                if let Some(section_title) = field.section_title() {
                    row = row.with_section_title(section_title);
                }
                Ok(row)
            })
            .collect()
    }
}
