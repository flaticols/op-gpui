# op-sdk

This repository contains `op-sdk`, an unofficial, UI-independent Rust adapter
for the desktop-app integration included with 1Password on macOS.

The library follows the same dynamic-library ABI and JSON request envelopes as
the official 1Password Go SDK. It intentionally does not depend on GPUI: an app
using upstream GPUI, a fork, or another UI toolkit can move its blocking SDK
calls to that runtime's background executor without creating incompatible UI
type dependencies.

See [`crates/op-sdk`](crates/op-sdk) for usage and compatibility details.

## Use from PMX

Cargo can fetch this private repository directly when the build machine has
read access through its SSH agent. Pin a commit in PMX's `Cargo.toml`:

```toml
op-sdk = {
    git = "ssh://git@github.com/flaticols/op-gpui.git",
    package = "op-sdk",
    rev = "<commit-sha>",
}
```

Alternatively, clone it beside PMX and use a path dependency while developing:

```sh
git clone git@github.com:flaticols/op-gpui.git ../op-gpui
```

```toml
op-sdk = { path = "../op-gpui/crates/op-sdk" }
```

If Cargo's built-in Git transport cannot use the machine's SSH setup, set
`net.git-fetch-with-cli = true` in that machine's Cargo configuration. No
registry publication is required; the crate has `publish = false`.

This project is not affiliated with or endorsed by 1Password. Use of the
1Password APIs and services is subject to the [1Password API Terms of
Service](https://developer.1password.com/terms/).
