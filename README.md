# op-gpui

This private workspace contains:

- `op-sdk`: a UI-independent Rust adapter for the desktop integration included
  with 1Password on macOS;
- `op-gpui`: a pure-GPUI, keyboard-first picker for vaults, items, fields, and
  stable `op://` references;
- `op-gpui-demo`: a runnable mock/live example application.

`op-sdk` follows the same dynamic-library ABI and JSON request envelopes as
the official 1Password Go SDK. It intentionally does not depend on GPUI: an app
using upstream GPUI, a fork, or another UI toolkit can move its blocking SDK
calls to that runtime's background executor without creating incompatible UI
type dependencies.

`op-gpui` depends only on `gpui` and `op-sdk`; it has no `gpui-component`
dependency. A PMX fork of `gpui-component` therefore does not affect its GPUI
type identity. See [`crates/op-sdk`](crates/op-sdk) and
[`crates/op-gpui`](crates/op-gpui) for their APIs and compatibility details.

## Use from GPUI

Cargo can fetch this private repository directly when the build machine has
read access through its SSH agent. Pin a commit in PMX's `Cargo.toml`:

```toml
op-sdk = {
    git = "ssh://git@github.com/flaticols/op-gpui.git",
    package = "op-sdk",
    rev = "<commit-sha>",
}
op-gpui = {
    git = "ssh://git@github.com/flaticols/op-gpui.git",
    package = "op-gpui",
    rev = "<same-commit-sha>",
}
```

Alternatively, clone it beside PMX and use a path dependency while developing:

```sh
git clone git@github.com:flaticols/op-gpui.git ../op-gpui
```

```toml
op-sdk = { path = "../op-gpui/crates/op-sdk" }
op-gpui = { path = "../op-gpui/crates/op-gpui" }
```

Cargo does not let a Git dependency inherit `[workspace.dependencies]` from
the consuming workspace. Instead, `op-gpui` declares the same upstream source
as PMX:

```toml
gpui = { version = "0.2.2", git = "https://github.com/zed-industries/zed" }
```

Because the source URL matches exactly, PMX's `Cargo.lock` selects one GPUI
revision for both PMX and `op-gpui`. This repository is currently locked and
tested at PMX's `7eec89207ccfbef7ba366da22fc885079a5c0296` revision. A workspace
that forks GPUI itself must redirect that upstream source consistently (for
example with a root `[patch]`) so only one GPUI source remains in its graph.

Applications that bootstrap GPUI through `gpui_platform` on macOS must enable
its `font-kit` feature. `gpui_platform` has no default features, and without
this feature `gpui_macos` deliberately renders no text:

```toml
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit"] }
```

## Run the demo

The default demo uses built-in metadata and does not contact 1Password:

```sh
cargo run -p op-gpui-demo -- --mock
```

To exercise the installed 1Password desktop app:

```sh
cargo run -p op-gpui-demo -- --account my.1password.com
```

Use `--library /path/to/libop_sdk_ipc_client.dylib` to override automatic
discovery. The picker emits a stable reference; it never loads or renders the
secret's plaintext.

If Cargo's built-in Git transport cannot use the machine's SSH setup, set
`net.git-fetch-with-cli = true` in that machine's Cargo configuration. No
registry publication is required; the crate has `publish = false`.

This project is not affiliated with or endorsed by 1Password. Use of the
1Password APIs and services is subject to the [1Password API Terms of
Service](https://developer.1password.com/terms/).
