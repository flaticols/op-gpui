# op-sdk

`op-sdk` is an unofficial Rust adapter for the desktop-app integration shipped
inside 1Password for macOS. It dynamically loads the library from an installed
1Password application; it does not bundle or redistribute 1Password software.
The crate is intended to be consumed from a private clone through a Cargo path
or SSH Git dependency and is not configured for registry publication.

The desktop SDK protocol is private and may change between 1Password releases.
Applications should surface connection and compatibility errors to users.

```rust,no_run
use op_sdk::Client;

let client = Client::builder()
    .desktop("my.1password.com")
    .integration("Example app", env!("CARGO_PKG_VERSION"))
    .connect()?;

for vault in client.vaults()? {
    println!("{}", vault.title());
}
# Ok::<(), op_sdk::Error>(())
```

## Requirements

- macOS on Apple silicon or Intel
- 1Password desktop app installed in `/Applications` or `~/Applications`
- **Settings → Developer → Integrate with other apps** enabled
- An account name or account UUID

Calls are synchronous because the dylib ABI is synchronous and does not expose
cancellation. GUI applications should execute calls on their background
executor and apply results back on the UI thread.

`Client::fields` deliberately returns field metadata only. `Client::resolve`
returns a `SecretValue`, whose allocation is cleared on drop and whose
`Debug`/`Display` output is always redacted. Plaintext is available only through
the explicit `SecretValue::expose_secret` method.

## Compatibility

The implementation mirrors commit
[`5866f431`](https://github.com/1Password/onepassword-sdk-go/tree/5866f43111ffeee5952e43a13da1aafef98200c8)
of the official Go SDK. The desktop SDK message protocol is private, so unknown
remote error names are preserved and exposed rather than collapsed.

This project is not affiliated with or endorsed by 1Password. Use of the
1Password APIs and services is subject to the [1Password API Terms of
Service](https://developer.1password.com/terms/).
