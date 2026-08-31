use std::{fmt, ops::Deref};

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{Error, Result};

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self> {
                let value = value.into();
                validate_reference_segment(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl Deref for $name {
            type Target = str;

            fn deref(&self) -> &Self::Target {
                self.as_str()
            }
        }
    };
}

id_type!(VaultId);
id_type!(ItemId);
id_type!(FieldId);
id_type!(SectionId);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[non_exhaustive]
pub struct VaultOverview {
    id: VaultId,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "activeItemCount")]
    active_item_count: u32,
}

impl VaultOverview {
    pub fn id(&self) -> &VaultId {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn active_item_count(&self) -> u32 {
        self.active_item_count
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[non_exhaustive]
pub struct ItemOverview {
    id: ItemId,
    title: String,
    category: ItemCategory,
    #[serde(rename = "vaultId")]
    vault_id: VaultId,
}

impl ItemOverview {
    pub fn id(&self) -> &ItemId {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn category(&self) -> &ItemCategory {
        &self.category
    }

    pub fn vault_id(&self) -> &VaultId {
        &self.vault_id
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ItemCategory(String);

impl ItemCategory {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ItemCategory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FieldKind(String);

impl FieldKind {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FieldKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct FieldOverview {
    id: FieldId,
    title: String,
    section_id: Option<SectionId>,
    section_title: Option<String>,
    kind: FieldKind,
}

impl FieldOverview {
    pub(crate) fn new(
        id: FieldId,
        title: String,
        section_id: Option<SectionId>,
        section_title: Option<String>,
        kind: FieldKind,
    ) -> Self {
        Self {
            id,
            title,
            section_id,
            section_title,
            kind,
        }
    }

    pub fn id(&self) -> &FieldId {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn section_id(&self) -> Option<&SectionId> {
        self.section_id.as_ref()
    }

    pub fn section_title(&self) -> Option<&str> {
        self.section_title.as_deref()
    }

    pub fn kind(&self) -> &FieldKind {
        &self.kind
    }
}

#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SecretReference(String);

impl SecretReference {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let path_and_query = value
            .strip_prefix("op://")
            .ok_or_else(|| Error::InvalidSecretReference(value.clone()))?;
        if path_and_query.contains('#') {
            return Err(Error::InvalidSecretReference(value));
        }
        let (path, query) = path_and_query
            .split_once('?')
            .map_or((path_and_query, None), |(path, query)| (path, Some(query)));
        if query.is_some_and(str::is_empty) {
            return Err(Error::InvalidSecretReference(value));
        }
        let segments = path.split('/').collect::<Vec<_>>();
        if !(segments.len() == 3 || segments.len() == 4) {
            return Err(Error::InvalidSecretReference(value));
        }
        for segment in segments {
            validate_reference_segment(segment)?;
        }
        Ok(Self(value))
    }

    pub fn for_field(vault_id: &VaultId, item_id: &ItemId, field: &FieldOverview) -> Result<Self> {
        let value = match field.section_id() {
            Some(section_id) => format!("op://{vault_id}/{item_id}/{section_id}/{}", field.id()),
            None => format!("op://{vault_id}/{item_id}/{}", field.id()),
        };
        Self::parse(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("SecretReference")
            .field(&self.0)
            .finish()
    }
}

impl fmt::Display for SecretReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A plaintext secret that clears its allocation on drop and redacts formatting.
pub struct SecretValue(Zeroizing<String>);

impl SecretValue {
    pub(crate) fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    /// Explicitly expose the plaintext value.
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

impl fmt::Display for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

fn validate_reference_segment(segment: &str) -> Result<()> {
    if segment.is_empty()
        || segment
            .chars()
            .any(|character| matches!(character, '/' | '?' | '#'))
    {
        return Err(Error::InvalidReferenceSegment {
            segment: segment.to_owned(),
        });
    }
    Ok(())
}
