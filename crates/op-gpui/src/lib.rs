//! Pure GPUI components for selecting stable 1Password secret references.
//!
//! This crate deliberately depends on `gpui` directly and does not depend on
//! `gpui-component`. Applications using a component-library fork can embed the
//! picker as a normal GPUI entity and map their theme into [`PickerTheme`].

mod picker;
mod provider;
mod theme;

pub use picker::{PickerLevel, SecretPicker, SecretPickerEvent, init};
pub use provider::{
    Field, Item, OnePasswordProvider, ProviderError, ProviderResult, SecretProvider, Vault,
};
pub use theme::PickerTheme;
