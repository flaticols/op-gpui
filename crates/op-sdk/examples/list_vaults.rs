use std::env;

use op_sdk::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let account = env::var("OP_ACCOUNT_NAME")
        .map_err(|_| "set OP_ACCOUNT_NAME to your 1Password account name or UUID")?;
    let client = Client::builder()
        .desktop(account)
        .integration("op-sdk example", env!("CARGO_PKG_VERSION"))
        .connect()?;

    for vault in client.vaults()? {
        println!("{} ({})", vault.title(), vault.id());
    }
    client.close()?;
    Ok(())
}
