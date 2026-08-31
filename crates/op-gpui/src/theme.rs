use gpui::{Rgba, rgb};

/// Presentation tokens used by [`crate::SecretPicker`].
///
/// The picker has no dependency on a component library. Applications can map
/// their own theme into this record and update the picker when appearance
/// changes.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct PickerTheme {
    /// Window or panel background.
    pub background: Rgba,
    /// Raised search and row surface.
    pub surface: Rgba,
    /// Primary text.
    pub foreground: Rgba,
    /// Supporting text.
    pub muted_foreground: Rgba,
    /// Hairline boundaries.
    pub border: Rgba,
    /// Pointer hover surface.
    pub hover: Rgba,
    /// Persistent keyboard selection surface.
    pub selected: Rgba,
    /// Focus-visible outline and small accent marks.
    pub accent: Rgba,
    /// Error text and boundary.
    pub danger: Rgba,
}

impl Default for PickerTheme {
    fn default() -> Self {
        Self {
            background: rgb(0x111318),
            surface: rgb(0x191c23),
            foreground: rgb(0xf4f4f5),
            muted_foreground: rgb(0xa1a1aa),
            border: rgb(0x30343d),
            hover: rgb(0x222630),
            selected: rgb(0x25334d),
            accent: rgb(0x6ea8fe),
            danger: rgb(0xff7b72),
        }
    }
}
