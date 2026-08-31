use std::{env, path::PathBuf, sync::Arc};

use gpui::{
    App, AppContext as _, Bounds, Context, Entity, FontWeight, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Window, WindowBounds, WindowOptions, div, px, rems, rgb,
    size,
};
use gpui_platform::application;
use op_gpui::{
    Field, Item, OnePasswordProvider, ProviderError, ProviderResult, SecretPicker,
    SecretPickerEvent, SecretProvider, Vault,
};
use op_sdk::SecretReference;

struct Demo {
    picker: Entity<SecretPicker>,
    source: SharedString,
    selected: Option<SecretReference>,
    status: SharedString,
}

impl Demo {
    fn new(
        provider: Arc<dyn SecretProvider>,
        source: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let picker = cx.new(|cx| SecretPicker::new(provider, cx));
        cx.subscribe(
            &picker,
            |this, _, event: &SecretPickerEvent, cx| match event {
                SecretPickerEvent::Selected(reference) => {
                    this.selected = Some(reference.clone());
                    this.status = "Reference selected; plaintext was not loaded".into();
                    cx.notify();
                }
                SecretPickerEvent::CancelRequested => {
                    this.status = "Cancel requested at the vault list".into();
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        let focus_handle = picker.read(cx).focus_handle();
        window.focus(&focus_handle, cx);

        Self {
            picker,
            source,
            selected: None,
            status: "Navigate with the keyboard or pointer".into(),
        }
    }
}

impl Render for Demo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self
            .selected
            .as_ref()
            .map_or_else(|| "No reference selected".to_owned(), ToString::to_string);

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_4()
            .p_6()
            .bg(rgb(0x0b0d11))
            .text_color(rgb(0xf4f4f5))
            .child(
                div()
                    .w(rems(34.))
                    .flex()
                    .items_end()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("op-gpui demo"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0xa1a1aa))
                                    .child(self.status.clone()),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0xa1a1aa))
                            .child(self.source.clone()),
                    ),
            )
            .child(
                div()
                    .w(rems(34.))
                    .h(rems(34.))
                    .min_h_0()
                    .child(self.picker.clone()),
            )
            .child(
                div()
                    .w(rems(34.))
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0x30343d))
                    .bg(rgb(0x111318))
                    .text_sm()
                    .child(selected),
            )
    }
}

#[derive(Default)]
struct MockProvider;

impl SecretProvider for MockProvider {
    fn vaults(&self) -> ProviderResult<Vec<Vault>> {
        Ok(vec![
            Vault::new("personal", "Personal")
                .with_description("Private credentials")
                .with_active_item_count(2),
            Vault::new("engineering", "Engineering")
                .with_description("Shared services")
                .with_active_item_count(2),
        ])
    }

    fn items(&self, vault_id: &str) -> ProviderResult<Vec<Item>> {
        match vault_id {
            "personal" => Ok(vec![
                Item::new(vault_id, "email", "Email account").with_category("Login"),
                Item::new(vault_id, "router", "Home router").with_category("Password"),
            ]),
            "engineering" => Ok(vec![
                Item::new(vault_id, "database", "Production database").with_category("Database"),
                Item::new(vault_id, "deploy", "Deployment token").with_category("API credential"),
            ]),
            _ => Err(ProviderError::new(format!("Unknown vault: {vault_id}"))),
        }
    }

    fn fields(&self, vault_id: &str, item_id: &str) -> ProviderResult<Vec<Field>> {
        let fields = match (vault_id, item_id) {
            ("personal", "email") => vec![
                field(vault_id, item_id, "username", "Username", "string")?,
                field(vault_id, item_id, "password", "Password", "concealed")?,
            ],
            ("personal", "router") => vec![field(
                vault_id,
                item_id,
                "password",
                "Admin password",
                "concealed",
            )?],
            ("engineering", "database") => vec![
                field(vault_id, item_id, "host", "Host", "string")?
                    .with_section_title("Connection"),
                field(vault_id, item_id, "password", "Password", "concealed")?
                    .with_section_title("Credentials"),
            ],
            ("engineering", "deploy") => vec![field(
                vault_id,
                item_id,
                "credential",
                "Credential",
                "concealed",
            )?],
            _ => {
                return Err(ProviderError::new(format!(
                    "Unknown item: {vault_id}/{item_id}"
                )));
            }
        };
        Ok(fields)
    }
}

fn field(
    vault_id: &str,
    item_id: &str,
    field_id: &str,
    title: &str,
    kind: &str,
) -> ProviderResult<Field> {
    let reference = SecretReference::parse(format!("op://{vault_id}/{item_id}/{field_id}"))
        .map_err(ProviderError::from)?;
    Ok(Field::new(field_id, title, reference).with_kind(kind))
}

struct Options {
    account: Option<String>,
    library: Option<PathBuf>,
}

fn options() -> Result<Options, String> {
    let mut account = None;
    let mut library = None;
    let mut arguments = env::args().skip(1);

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--mock" => account = None,
            "--account" => {
                account = Some(
                    arguments
                        .next()
                        .ok_or_else(|| "--account requires a name or UUID".to_owned())?,
                );
            }
            "--library" => {
                library =
                    Some(PathBuf::from(arguments.next().ok_or_else(|| {
                        "--library requires a dylib path".to_owned()
                    })?));
            }
            "-h" | "--help" => {
                println!(
                    "op-gpui-demo\n\n  --mock                 use built-in sample data (default)\n  --account <name|uuid>  connect to the 1Password desktop app\n  --library <path>       override libop_sdk_ipc_client.dylib discovery"
                );
                std::process::exit(0);
            }
            unknown => return Err(format!("Unknown argument: {unknown}")),
        }
    }

    Ok(Options { account, library })
}

fn provider() -> Result<(Arc<dyn SecretProvider>, SharedString), String> {
    let options = options()?;
    let Some(account) = options.account else {
        return Ok((Arc::new(MockProvider), "mock catalogue".into()));
    };

    let mut builder = op_sdk::Client::builder()
        .desktop(&account)
        .integration("op-gpui-demo", env!("CARGO_PKG_VERSION"));
    if let Some(path) = options.library {
        builder = builder.library_path(path);
    }
    let client = builder
        .connect()
        .map_err(|error| format!("Couldn’t connect to 1Password: {error}"))?;
    Ok((
        Arc::new(OnePasswordProvider::new(client)),
        format!("1Password · {account}").into(),
    ))
}

fn main() {
    let (provider, source) = provider().unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });

    application().run(move |cx: &mut App| {
        op_gpui::init(cx);
        let bounds = Bounds::centered(None, size(px(720.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            {
                let provider = Arc::clone(&provider);
                let source = source.clone();
                move |window, cx| cx.new(|cx| Demo::new(provider, source, window, cx))
            },
        )
        .expect("failed to open the demo window");
        cx.activate(true);
    });
}
