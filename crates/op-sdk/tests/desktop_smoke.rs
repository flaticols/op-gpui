use op_sdk::Client;

/// This test opens the real desktop-app authorization flow and is never run in CI.
#[test]
#[ignore = "requires an installed, configured 1Password app and OP_ACCOUNT_NAME"]
fn connects_to_installed_desktop_app() -> Result<(), Box<dyn std::error::Error>> {
    let account = std::env::var("OP_ACCOUNT_NAME")?;
    let client = Client::builder()
        .desktop(account)
        .integration("op-sdk smoke test", env!("CARGO_PKG_VERSION"))
        .connect()?;
    let _vaults = client.vaults()?;
    client.close()?;
    Ok(())
}
