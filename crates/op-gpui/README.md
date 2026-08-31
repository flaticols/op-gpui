# op-gpui

`op-gpui` is a pure-GPUI picker for stable 1Password `op://` references. It
owns background loading, vault → item → field navigation, type-to-filter,
keyboard actions, focus, accessibility metadata, and a virtualized row list.
It does not depend on `gpui-component`.

```rust,no_run
use std::sync::Arc;

use gpui::{AppContext as _, Context, Entity, Window};
use op_gpui::{OnePasswordProvider, SecretPicker, SecretPickerEvent, SecretProvider};

struct Host {
    picker: Entity<SecretPicker>,
}

impl Host {
    fn new(client: op_sdk::Client, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let provider: Arc<dyn SecretProvider> =
            Arc::new(OnePasswordProvider::new(client));
        let picker = cx.new(|cx| SecretPicker::new(provider, cx));

        cx.subscribe(&picker, |_host, _picker, event, _cx| {
            if let SecretPickerEvent::Selected(reference) = event {
                // Resolve only where the secret is actually needed.
                println!("selected {reference}");
            }
        })
        .detach();

        let focus = picker.read(cx).focus_handle();
        window.focus(&focus, cx);
        Self { picker }
    }
}
```

Call `op_gpui::init(cx)` once during application bootstrap to install the
contextual key bindings. Render the retained picker entity wherever it belongs
in the host layout.

## Interaction

- Type to filter the current list.
- Up/Down and Home/End move the selection.
- Enter opens a vault/item or confirms a field.
- Escape clears a query, moves to the parent, or emits `CancelRequested` at
  the root.
- Left moves to the parent; Backspace erases the query or moves to the parent
  when the query is empty.
- Command-R or Control-R retries the current request.

The provider API returns metadata and stable references only. Plaintext remains
behind `op_sdk::Client::resolve` and `SecretValue::expose_secret`.

## GPUI source identity

Rust treats types from two Git sources or revisions as different types, even
when their names are identical. This crate declares the same unpinned upstream
source URL as PMX so the consuming workspace's lockfile selects the revision.
The repository lockfile currently verifies PMX's `7eec892` revision. A fork of
`gpui-component` is unrelated because this crate never depends on it; a fork of
GPUI itself must be applied consistently to the whole dependency graph.
