use std::sync::Arc;

use gpui::{
    AnyElement, App, AppContext as _, Context, EventEmitter, FocusHandle, FontWeight,
    InteractiveElement as _, IntoElement, KeyBinding, KeyDownEvent, ParentElement as _, Render,
    Role, ScrollStrategy, SharedString, StatefulInteractiveElement as _, Styled as _, Task,
    UniformListScrollHandle, Window, actions, div, prelude::FluentBuilder as _, uniform_list,
};
use op_sdk::SecretReference;

use crate::{Field, Item, PickerTheme, ProviderError, SecretProvider, Vault};

const KEY_CONTEXT: &str = "OnePasswordSecretPicker";

actions!(
    op_secret_picker,
    [
        SelectPrevious,
        SelectNext,
        SelectFirst,
        SelectLast,
        Confirm,
        Cancel,
        GoBack,
        EraseQueryCharacter,
        Retry,
    ]
);

/// Registers the picker's contextual key bindings.
///
/// Call this once while bootstrapping the GPUI application.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectPrevious, Some(KEY_CONTEXT)),
        KeyBinding::new("down", SelectNext, Some(KEY_CONTEXT)),
        KeyBinding::new("home", SelectFirst, Some(KEY_CONTEXT)),
        KeyBinding::new("end", SelectLast, Some(KEY_CONTEXT)),
        KeyBinding::new("enter", Confirm, Some(KEY_CONTEXT)),
        KeyBinding::new("escape", Cancel, Some(KEY_CONTEXT)),
        KeyBinding::new("left", GoBack, Some(KEY_CONTEXT)),
        KeyBinding::new("backspace", EraseQueryCharacter, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-r", Retry, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-r", Retry, Some(KEY_CONTEXT)),
    ]);
}

/// The current navigation level of a [`SecretPicker`].
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PickerLevel {
    /// The root vault list.
    Vaults,
    /// The items inside one vault.
    Items {
        /// Stable vault identifier.
        vault_id: String,
        /// Vault title used in breadcrumbs.
        vault_title: String,
    },
    /// The fields inside one item.
    Fields {
        /// Stable vault identifier.
        vault_id: String,
        /// Vault title used in breadcrumbs.
        vault_title: String,
        /// Stable item identifier.
        item_id: String,
        /// Item title used in breadcrumbs.
        item_title: String,
    },
}

/// Semantic events emitted by [`SecretPicker`].
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SecretPickerEvent {
    /// The user confirmed a field's stable `op://` reference.
    Selected(SecretReference),
    /// Escape was pressed at the root with an empty query.
    CancelRequested,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Row {
    Vault(Vault),
    Item(Item),
    Field(Field),
}

impl Row {
    fn title(&self) -> &str {
        match self {
            Self::Vault(vault) => vault.title(),
            Self::Item(item) => item.title(),
            Self::Field(field) => field.title(),
        }
    }

    fn detail(&self) -> String {
        match self {
            Self::Vault(vault) => {
                if !vault.description().is_empty() {
                    vault.description().to_owned()
                } else if let Some(count) = vault.active_item_count() {
                    format!("{count} active items")
                } else {
                    String::new()
                }
            }
            Self::Item(item) => item.category().to_owned(),
            Self::Field(field) => match (field.section_title(), field.kind().is_empty()) {
                (Some(section), false) => format!("{section} · {}", field.kind()),
                (Some(section), true) => section.to_owned(),
                (None, false) => field.kind().to_owned(),
                (None, true) => String::new(),
            },
        }
    }

    fn stable_element_id(&self) -> String {
        match self {
            Self::Vault(vault) => format!("op-picker:vault:{}", vault.id()),
            Self::Item(item) => format!("op-picker:item:{}:{}", item.vault_id(), item.id()),
            Self::Field(field) => format!("op-picker:field:{}", field.reference()),
        }
    }

    fn has_children(&self) -> bool {
        !matches!(self, Self::Field(_))
    }
}

#[derive(Clone, Debug)]
enum LoadRequest {
    Vaults,
    Items(Vault),
    Fields { vault: Vault, item: Item },
}

impl LoadRequest {
    fn level(&self) -> PickerLevel {
        match self {
            Self::Vaults => PickerLevel::Vaults,
            Self::Items(vault) => PickerLevel::Items {
                vault_id: vault.id().to_owned(),
                vault_title: vault.title().to_owned(),
            },
            Self::Fields { vault, item } => PickerLevel::Fields {
                vault_id: vault.id().to_owned(),
                vault_title: vault.title().to_owned(),
                item_id: item.id().to_owned(),
                item_title: item.title().to_owned(),
            },
        }
    }

    fn run(&self, provider: &dyn SecretProvider) -> Result<Vec<Row>, ProviderError> {
        match self {
            Self::Vaults => provider
                .vaults()
                .map(|vaults| vaults.into_iter().map(Row::Vault).collect()),
            Self::Items(vault) => provider
                .items(vault.id())
                .map(|items| items.into_iter().map(Row::Item).collect()),
            Self::Fields { vault, item } => provider
                .fields(vault.id(), item.id())
                .map(|fields| fields.into_iter().map(Row::Field).collect()),
        }
    }
}

#[derive(Clone, Debug)]
enum LoadState {
    Loading,
    Ready,
    Failed(ProviderError),
}

/// A retained, keyboard-first picker that emits stable 1Password references.
///
/// The picker owns navigation, filtering, focus, virtualization, and async
/// loading. Its [`SecretProvider`] owns data access, and the embedding
/// application owns the selected reference after receiving
/// [`SecretPickerEvent::Selected`].
pub struct SecretPicker {
    provider: Arc<dyn SecretProvider>,
    theme: PickerTheme,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    level: PickerLevel,
    request: LoadRequest,
    load_state: LoadState,
    rows: Vec<Row>,
    filtered_rows: Vec<usize>,
    selected_ix: Option<usize>,
    query: String,
    generation: u64,
    _load_task: Option<Task<()>>,
}

impl EventEmitter<SecretPickerEvent> for SecretPicker {}

impl SecretPicker {
    /// Creates a picker and immediately loads its vault list in the background.
    pub fn new(provider: Arc<dyn SecretProvider>, cx: &mut Context<Self>) -> Self {
        Self::with_theme(provider, PickerTheme::default(), cx)
    }

    /// Creates a picker with application-supplied presentation tokens.
    pub fn with_theme(
        provider: Arc<dyn SecretProvider>,
        theme: PickerTheme,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut picker = Self {
            provider,
            theme,
            focus_handle: cx.focus_handle(),
            scroll_handle: UniformListScrollHandle::new(),
            level: PickerLevel::Vaults,
            request: LoadRequest::Vaults,
            load_state: LoadState::Loading,
            rows: Vec::new(),
            filtered_rows: Vec::new(),
            selected_ix: None,
            query: String::new(),
            generation: 0,
            _load_task: None,
        };
        picker.load(LoadRequest::Vaults, cx);
        picker
    }

    /// Returns a focus handle suitable for focusing the picker after mounting.
    pub fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    /// Returns the current navigation level.
    pub fn level(&self) -> &PickerLevel {
        &self.level
    }

    /// Returns the active type-to-filter query.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Replaces presentation tokens and redraws the picker.
    pub fn set_theme(&mut self, theme: PickerTheme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    /// Reloads the current navigation level.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.load(self.request.clone(), cx);
    }

    fn load(&mut self, request: LoadRequest, cx: &mut Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.level = request.level();
        self.request = request.clone();
        self.load_state = LoadState::Loading;
        self.rows.clear();
        self.filtered_rows.clear();
        self.selected_ix = None;
        self.query.clear();
        cx.notify();

        let provider = Arc::clone(&self.provider);
        let completed_request = request.clone();
        let load = cx.background_spawn(async move { request.run(provider.as_ref()) });
        self._load_task = Some(cx.spawn(async move |this, cx| {
            let result = load.await;
            _ = this.update(cx, |this, cx| {
                if this.generation != generation
                    || this.request.level() != completed_request.level()
                {
                    return;
                }

                match result {
                    Ok(rows) => {
                        this.rows = rows;
                        this.load_state = LoadState::Ready;
                        this.rebuild_filter();
                    }
                    Err(error) => {
                        this.rows.clear();
                        this.filtered_rows.clear();
                        this.selected_ix = None;
                        this.load_state = LoadState::Failed(error);
                    }
                }
                cx.notify();
            });
        }));
    }

    fn rebuild_filter(&mut self) {
        self.filtered_rows = filtered_row_indices(&self.rows, &self.query);
        self.selected_ix = (!self.filtered_rows.is_empty()).then_some(0);
        if self.selected_ix.is_some() {
            self.scroll_handle
                .scroll_to_item(0, ScrollStrategy::Nearest);
        }
    }

    fn select_relative(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.filtered_rows.len();
        if count == 0 {
            return;
        }
        let current = self.selected_ix.unwrap_or(0);
        let next = current.saturating_add_signed(delta).min(count - 1);
        if next != current || self.selected_ix.is_none() {
            self.selected_ix = Some(next);
            self.scroll_handle
                .scroll_to_item(next, ScrollStrategy::Nearest);
            cx.notify();
        }
    }

    fn select_boundary(&mut self, last: bool, cx: &mut Context<Self>) {
        if self.filtered_rows.is_empty() {
            return;
        }
        let next = if last {
            self.filtered_rows.len() - 1
        } else {
            0
        };
        self.selected_ix = Some(next);
        self.scroll_handle
            .scroll_to_item(next, ScrollStrategy::Nearest);
        cx.notify();
    }

    fn selected_row(&self) -> Option<&Row> {
        let source_ix = *self.filtered_rows.get(self.selected_ix?)?;
        self.rows.get(source_ix)
    }

    fn activate_selected(&mut self, cx: &mut Context<Self>) {
        if matches!(self.load_state, LoadState::Failed(_)) {
            self.refresh(cx);
            return;
        }

        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        match row {
            Row::Vault(vault) => self.load(LoadRequest::Items(vault), cx),
            Row::Item(item) => {
                let LoadRequest::Items(vault) = self.request.clone() else {
                    return;
                };
                self.load(LoadRequest::Fields { vault, item }, cx);
            }
            Row::Field(field) => cx.emit(SecretPickerEvent::Selected(field.reference().clone())),
        }
    }

    fn activate_visible_ix(&mut self, visible_ix: usize, cx: &mut Context<Self>) {
        if visible_ix >= self.filtered_rows.len() {
            return;
        }
        self.selected_ix = Some(visible_ix);
        self.activate_selected(cx);
        cx.notify();
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        match self.request.clone() {
            LoadRequest::Vaults => {}
            LoadRequest::Items(_) => self.load(LoadRequest::Vaults, cx),
            LoadRequest::Fields { vault, .. } => self.load(LoadRequest::Items(vault), cx),
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if !self.query.is_empty() {
            self.query.clear();
            self.rebuild_filter();
            cx.notify();
        } else if !matches!(self.request, LoadRequest::Vaults) {
            self.go_back(cx);
        } else {
            cx.emit(SecretPickerEvent::CancelRequested);
        }
    }

    fn erase_query_character(&mut self, cx: &mut Context<Self>) {
        if self.query.pop().is_some() {
            self.rebuild_filter();
            cx.notify();
        } else {
            self.go_back(cx);
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.modifiers.control || event.keystroke.modifiers.platform {
            return;
        }
        let Some(text) = event.keystroke.key_char.as_deref() else {
            return;
        };
        if text.chars().any(char::is_control) {
            return;
        }

        self.query.push_str(text);
        self.rebuild_filter();
        cx.stop_propagation();
        cx.notify();
    }

    fn on_select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.select_relative(-1, cx);
    }

    fn on_select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.select_relative(1, cx);
    }

    fn on_select_first(&mut self, _: &SelectFirst, _: &mut Window, cx: &mut Context<Self>) {
        self.select_boundary(false, cx);
    }

    fn on_select_last(&mut self, _: &SelectLast, _: &mut Window, cx: &mut Context<Self>) {
        self.select_boundary(true, cx);
    }

    fn on_confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        self.activate_selected(cx);
    }

    fn on_cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        self.cancel(cx);
    }

    fn on_go_back(&mut self, _: &GoBack, _: &mut Window, cx: &mut Context<Self>) {
        self.go_back(cx);
    }

    fn on_erase_query_character(
        &mut self,
        _: &EraseQueryCharacter,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.erase_query_character(cx);
    }

    fn on_retry(&mut self, _: &Retry, _: &mut Window, cx: &mut Context<Self>) {
        self.refresh(cx);
    }

    fn level_title(&self) -> &str {
        match &self.level {
            PickerLevel::Vaults => "Choose a vault",
            PickerLevel::Items { .. } => "Choose an item",
            PickerLevel::Fields { .. } => "Choose a field",
        }
    }

    fn breadcrumb(&self) -> String {
        match &self.level {
            PickerLevel::Vaults => "1Password".to_owned(),
            PickerLevel::Items { vault_title, .. } => format!("1Password  /  {vault_title}"),
            PickerLevel::Fields {
                vault_title,
                item_title,
                ..
            } => format!("1Password  /  {vault_title}  /  {item_title}"),
        }
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let can_go_back = !matches!(self.level, PickerLevel::Vaults);
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(can_go_back, |header| {
                        header.child(
                            div()
                                .id("op-picker-back")
                                .role(Role::Button)
                                .aria_label("Back")
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(theme.surface)
                                .hover(move |style| style.bg(theme.hover))
                                .on_click(cx.listener(|this, _, _, cx| this.go_back(cx)))
                                .child("‹"),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .truncate()
                            .child(self.breadcrumb()),
                    ),
            )
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(self.level_title())),
            )
            .child(
                div()
                    .id("op-picker-query")
                    .aria_label("Type to filter 1Password entries")
                    .flex()
                    .items_center()
                    .h_10()
                    .px_3()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .text_sm()
                    .when(self.query.is_empty(), |query| {
                        query.text_color(theme.muted_foreground)
                    })
                    .child(if self.query.is_empty() {
                        SharedString::from("Type to filter…")
                    } else {
                        SharedString::from(format!("{}▏", self.query))
                    }),
            )
    }

    fn render_row(&mut self, visible_ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let source_ix = self.filtered_rows[visible_ix];
        let row = &self.rows[source_ix];
        let theme = self.theme;
        let selected = self.selected_ix == Some(visible_ix);
        let title = SharedString::from(row.title().to_owned());
        let detail = SharedString::from(row.detail());
        let has_detail = !detail.is_empty();
        let has_children = row.has_children();
        let aria_label = if has_detail {
            SharedString::from(format!("{title}, {detail}"))
        } else {
            title.clone()
        };

        div()
            .id(row.stable_element_id())
            .role(Role::ListItem)
            .aria_label(aria_label)
            .aria_selected(selected)
            .aria_position_in_set(visible_ix + 1)
            .aria_size_of_set(self.filtered_rows.len())
            .h_12()
            .w_full()
            .px_4()
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(theme.border)
            .when(selected, |row| row.bg(theme.selected))
            .when(!selected, |row| {
                row.hover(move |style| style.bg(theme.hover))
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.activate_visible_ix(visible_ix, cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().truncate().child(title))
                    .when(has_detail, |content| {
                        content.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .truncate()
                                .child(detail),
                        )
                    }),
            )
            .when(has_children, |row| {
                row.child(
                    div()
                        .text_color(theme.muted_foreground)
                        .text_lg()
                        .child("›"),
                )
            })
            .into_any_element()
    }

    fn render_body(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        match &self.load_state {
            LoadState::Loading => div()
                .id("op-picker-loading")
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child("Loading…")
                .into_any_element(),
            LoadState::Failed(error) => {
                let message = SharedString::from(error.message().to_owned());
                div()
                    .id("op-picker-error")
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .p_6()
                    .text_center()
                    .child(
                        div()
                            .text_color(theme.danger)
                            .child("Couldn’t load 1Password"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(message),
                    )
                    .child(
                        div()
                            .id("op-picker-retry")
                            .role(Role::Button)
                            .aria_label("Retry")
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.danger)
                            .hover(move |style| style.bg(theme.hover))
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx)))
                            .child("Retry"),
                    )
                    .into_any_element()
            }
            LoadState::Ready if self.filtered_rows.is_empty() => div()
                .id("op-picker-empty")
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .text_color(theme.muted_foreground)
                .child(if self.query.is_empty() {
                    "No entries".to_owned()
                } else {
                    format!("No matches for “{}”", self.query)
                })
                .into_any_element(),
            LoadState::Ready => div()
                .id("op-picker-list-region")
                .role(Role::List)
                .aria_label(SharedString::from(self.level_title()))
                .flex_1()
                .min_h_0()
                .child(
                    uniform_list(
                        "op-picker-list",
                        self.filtered_rows.len(),
                        cx.processor(|this, range: std::ops::Range<usize>, _window, cx| {
                            range
                                .map(|visible_ix| this.render_row(visible_ix, cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&self.scroll_handle)
                    .size_full(),
                )
                .into_any_element(),
        }
    }
}

fn filtered_row_indices(rows: &[Row], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    rows.iter()
        .enumerate()
        .filter_map(|(ix, row)| {
            let matches = query.is_empty()
                || row.title().to_lowercase().contains(&query)
                || row.detail().to_lowercase().contains(&query);
            matches.then_some(ix)
        })
        .collect()
}

impl Render for SecretPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .id("op-secret-picker")
            .role(Role::Group)
            .aria_label("1Password secret picker")
            .track_focus(&self.focus_handle)
            .key_context(KEY_CONTEXT)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_action(cx.listener(Self::on_select_previous))
            .on_action(cx.listener(Self::on_select_next))
            .on_action(cx.listener(Self::on_select_first))
            .on_action(cx.listener(Self::on_select_last))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_go_back))
            .on_action(cx.listener(Self::on_erase_query_character))
            .on_action(cx.listener(Self::on_retry))
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .focus_visible(move |style| style.border_color(theme.accent))
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(self.render_header(cx))
            .child(self.render_body(cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_4()
                    .py_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("↑ ↓ navigate   enter choose")
                    .child("esc clear or back"),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use gpui::{TestAppContext, VisualTestContext, px};

    use super::*;

    fn row(title: &str, detail: &str) -> Row {
        Row::Vault(Vault::new(title, title).with_description(detail))
    }

    #[test]
    fn filtering_matches_titles_and_details() {
        let rows = vec![
            row("Personal", "Private credentials"),
            row("Engineering", "Shared services"),
        ];

        assert_eq!(filtered_row_indices(&rows, "service"), vec![1]);
    }

    #[test]
    fn filtering_is_case_insensitive() {
        let rows = vec![row("Engineering", "")];

        assert_eq!(filtered_row_indices(&rows, "ENGINE"), vec![0]);
    }

    struct TestProvider;

    impl SecretProvider for TestProvider {
        fn vaults(&self) -> crate::ProviderResult<Vec<Vault>> {
            Ok(vec![Vault::new("vault", "Vault")])
        }

        fn items(&self, vault_id: &str) -> crate::ProviderResult<Vec<Item>> {
            Ok(vec![Item::new(vault_id, "item", "Item")])
        }

        fn fields(&self, vault_id: &str, item_id: &str) -> crate::ProviderResult<Vec<Field>> {
            [
                ("username", "Username", "string"),
                ("password", "Password", "concealed"),
            ]
            .into_iter()
            .map(|(id, title, kind)| {
                let reference = SecretReference::parse(format!("op://{vault_id}/{item_id}/{id}"))
                    .map_err(ProviderError::from)?;
                Ok(Field::new(id, title, reference).with_kind(kind))
            })
            .collect()
        }
    }

    struct Host {
        picker: gpui::Entity<SecretPicker>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(400.)).h(px(500.)).child(self.picker.clone())
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }

    #[gpui::test]
    fn keyboard_navigation_emits_the_selected_reference(cx: &mut TestAppContext) {
        cx.update(init);
        let selected = Rc::new(RefCell::new(Vec::<SecretReference>::new()));
        let heard = Rc::clone(&selected);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let picker = cx.new(|cx| SecretPicker::new(Arc::new(TestProvider), cx));
            cx.subscribe(&picker, move |_, _, event: &SecretPickerEvent, _| {
                if let SecretPickerEvent::Selected(reference) = event {
                    heard.borrow_mut().push(reference.clone());
                }
            })
            .detach();
            let focus = picker.read(cx).focus_handle();
            window.focus(&focus, cx);
            Host { picker }
        });

        cx.run_until_parked();
        draw(cx);
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        draw(cx);
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        draw(cx);
        cx.simulate_keystrokes("down enter");
        cx.run_until_parked();

        assert_eq!(
            selected.borrow().as_slice(),
            [SecretReference::parse("op://vault/item/password").unwrap()]
        );
    }
}
