use std::collections::HashMap;
use iced::widget::{
    button, column, row, text, container, text_input, text_editor, image, svg, scrollable,
    checkbox, toggler, rule, pick_list, mouse_area, Space, Tooltip,
};
use iced::widget::tooltip::Position as TooltipPosition;

/// One option of a `<Select>`: `label` is shown, `value` is dispatched. Equality
/// (used by `pick_list` to mark the current selection) is by `value` only.
#[derive(Debug, Clone)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
}

impl SelectOption {
    /// Builds an option from a JSON array element: an object reads `label_field`/
    /// `value_field` (value falls back to label); a bare string is both.
    fn from_json(item: &serde_json::Value, label_field: &str, value_field: &str) -> Self {
        match item {
            serde_json::Value::Object(o) => {
                let get = |k: &str| o.get(k).map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                });
                let label = get(label_field).unwrap_or_default();
                let value = get(value_field).unwrap_or_else(|| label.clone());
                Self { label, value }
            }
            serde_json::Value::String(s) => Self { label: s.clone(), value: s.clone() },
            other => {
                let s = other.to_string();
                Self { label: s.clone(), value: s }
            }
        }
    }
}

impl std::fmt::Display for SelectOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

impl PartialEq for SelectOption {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

/// Stateful `text_editor` buffers, keyed by a `<TextArea>`'s `value` binding.
/// Owned by [`crate::GlacierUI`] and borrowed during render so the editors keep
/// their content/cursor across frames (glacier is otherwise stateless).
pub type EditorMap = HashMap<String, text_editor::Content>;
use iced::{Element, Length, Alignment, Color, Border, Padding, Background, Font, Gradient};
use iced::gradient::Linear;
use iced::Radians;
use crate::parser::{UiNode, NodeType};

/// Selects an `iced::Font` from a `font="..."` hint. `mono`/`monospace`/`code`
/// map to the monospaced font; anything else returns `None` (default font).
fn font_for(hint: &Option<String>) -> Option<Font> {
    match hint.as_deref().map(|s| s.to_ascii_lowercase()) {
        Some(ref s) if s == "mono" || s == "monospace" || s == "code" => Some(Font::MONOSPACE),
        Some(ref s) if s == "bold" => Some(Font { weight: iced::font::Weight::Bold, ..Default::default() }),
        _ => None,
    }
}

/// Whether a context string should count as "checked"/true.
fn is_truthy(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on" | "sim")
}

/// Maps `start`/`center`/`end` (and aliases) to a horizontal text alignment.
fn parse_text_align(s: &Option<String>) -> Option<iced::alignment::Horizontal> {
    use iced::alignment::Horizontal;
    match s.as_deref().map(|v| v.to_ascii_lowercase()) {
        Some(ref v) if v == "start" || v == "left" => Some(Horizontal::Left),
        Some(ref v) if v == "center" || v == "centre" => Some(Horizontal::Center),
        Some(ref v) if v == "end" || v == "right" => Some(Horizontal::Right),
        _ => None,
    }
}

/// Parses a gradient spec into an `iced::Gradient`.
///
/// Forms: `"#a #b"` (top→bottom, 180deg) or `"<angle> #a #b [#c ...]"` where
/// `<angle>` is in degrees (0 = upward). Needs at least two color stops.
fn parse_gradient(spec: &str) -> Option<Gradient> {
    let mut tokens = spec.split_whitespace().peekable();
    let mut angle_deg = 180.0_f32;
    // An optional leading numeric token is the angle in degrees.
    if let Some(first) = tokens.peek() {
        if !first.starts_with('#') {
            if let Ok(a) = first.trim_end_matches("deg").parse::<f32>() {
                angle_deg = a;
                tokens.next();
            }
        }
    }
    let colors: Vec<Color> = tokens.filter_map(parse_hex_color).collect();
    if colors.len() < 2 {
        return None;
    }
    let mut linear = Linear::new(Radians(angle_deg.to_radians()));
    let last = colors.len() - 1;
    for (i, c) in colors.into_iter().enumerate() {
        linear = linear.add_stop(i as f32 / last as f32, c);
    }
    Some(Gradient::Linear(linear))
}

/// Resolves the background of a node: a `gradient` wins over a solid `background`.
fn background_for(node: &UiNode) -> Option<Background> {
    if let Some(g) = node.gradient.as_ref().and_then(|s| parse_gradient(s)) {
        return Some(Background::Gradient(g));
    }
    node.background.as_ref().and_then(|bg| parse_hex_color(bg)).map(Background::Color)
}

#[derive(Debug, Clone)]
pub enum EngineMessage {
    UiClick(String),
    UiInputChanged { action: String, value: String },
    /// An edit on a `<TextArea>`: `binding` is its `value` key, `action` is the
    /// editor action to apply to the kept `Content`, `on_change` is the action
    /// dispatched (with the new full text) after applying it.
    UiEditorAction { binding: String, on_change: String, action: text_editor::Action, readonly: bool },
    /// Navigate to the given screen (button with `navigateTo`).
    Navigate(String),
    /// Go back to the previous screen (button with `navigateBack`).
    NavigateBack,
    FileChanged(String),
    /// Merge `(key, value)` pairs into the context and re-evaluate. Produced by
    /// component subscriptions (long-lived streams emitting raw `EngineMessage`);
    /// the host app just forwards it to [`crate::GlacierUI::dispatch`].
    ContextPatch(Vec<(String, String)>),
    /// An async effect ([`crate::component::Effect::Perform`]) completed with an
    /// [`crate::component::EffectOutcome`]: merge its `patch` into the context,
    /// show its `toast` (if any), and re-evaluate. Lets a `ctx.perform` future
    /// request a toast of its result the same way sync `update()` code does,
    /// without smuggling it through reserved context keys.
    EffectOutcome(crate::component::EffectOutcome),
    /// Mouse-press on a reorderable item's `dragHandle`. `order` is the full
    /// identity snapshot (every item's `reorderKey` value, in current order) at
    /// the moment the drag started; `key` is this item's own identity.
    DragStart { list: String, reorder_key: String, on_reorder: String, order: Vec<String>, key: String },
    /// Cursor entered another item of the same reorderable list while a drag is
    /// in progress — moves `key` to that item's position in the live order.
    DragHover { list: String, key: String },
    /// Left mouse button released anywhere (global subscription): ends the
    /// drag in progress, if any, dispatching `on_reorder` with the final order.
    DragEnd,
    /// Enter pressed inside a `formControl`-bound `TextInput` (see
    /// `UiNode::form_*`, hydrated by a `<Form>`'s evaluation in `eval.rs`).
    /// Always dispatches the enclosing `Form`'s `onSubmit` action — the
    /// component's `update()` decides what to do based on its own
    /// `glacier_ui::Form::is_valid()` — and, if there is a next control in the
    /// same form, also requests focus there (Tab-like), so the whole form can
    /// be filled with Enter alone. `next_focus` is the next input's stable id
    /// string (built by `form_input_id`), resolved into a real
    /// `iced::widget::text_input` focus by `GlacierUI::dispatch`.
    UiSubmit { action: String, next_focus: Option<String> },
    /// A button of the active [`crate::dialogs::DialogSpec`] was clicked.
    /// Closes the dialog; `action` is then routed to the owning component's
    /// `update()` just like a normal `UiClick`.
    DialogButton(String),
    /// The active dialog's backdrop was clicked (or another dismiss gesture
    /// fired). Only takes effect if the dialog is `dismissible`; closes it
    /// without routing any action.
    DialogDismiss,
    /// A toast's "×" button was clicked (see [`crate::toasts`]). Removes the
    /// toast with this id from the active list; never routed to a component.
    ToastDismiss(u64),
    /// Periodic tick (see [`crate::GlacierUI::toast_subscription`]) that
    /// prunes expired toasts from the active list. Carries no data of its
    /// own — the engine compares each toast's own timestamp against `now`.
    ToastTick,
    /// Tab / Shift+Tab pressed (global keyboard subscription): move focus to the
    /// next / previous focusable widget. iced's text inputs don't advance focus
    /// on Tab on their own, so the engine drives it (see `tab_focus_from_event`).
    FocusNext,
    FocusPrev,
    /// Window was resized (global window-event subscription): updates the
    /// engine's tracked viewport and re-evaluates so `@media` blocks re-resolve.
    /// `width`/`height` are logical px.
    Viewport { width: f32, height: f32 },
    /// A `fetch` requested by a component's Lua (`crate::lua`) finished: resume
    /// the suspended coroutine `id` on component `owner` with the HTTP result.
    LuauResume { owner: String, id: u64, result: crate::component::FetchResult },
    /// An event from a long-lived stream (`sse`/`websocket`) opened by a
    /// component's Lua: routed to `owner` so it can call the registered handler
    /// (`on_message`, …). `StreamEvent::Ready` also hands the engine the
    /// outbound command channel for WebSocket sends. See
    /// [`crate::GlacierUI::subscription`] and [`crate::net`].
    LuauStream { owner: String, id: u64, event: crate::net::StreamEvent },
    /// Um temporizador pedido por `after(ms, fn)` na Lua de um componente
    /// venceu: chama [`crate::Component::resume_timer`] com o `id` do
    /// temporizador em `owner`. Ver [`crate::component::PendingTimer`].
    LuauTimer { owner: String, id: u64 },
}

/// The stable focus id of a form-bound `TextInput`: `scope` is the enclosing
/// `<Form>`'s `"{owner}::{form name}"` prefix (shared by every control in that
/// form), `control` its own `formControl` name.
pub fn form_input_id(scope: &str, control: &str) -> String {
    format!("glacier_form::{scope}::{control}")
}

/// Helper to parse iced::Length from optional string.
///
/// Aceita: `fill`, `shrink`, um número (px fixo), e **pesos de flex**
/// `fill N` / `fill-N` → `Length::FillPortion(N)` (divide o espaço livre entre
/// irmãos proporcionalmente; `fill 2` ocupa o dobro de um `fill`). Tudo
/// case-insensitive. Valor inválido cai em `shrink`.
fn parse_length(s: &Option<String>) -> Length {
    let Some(raw) = s.as_deref() else {
        return Length::Shrink;
    };
    let v = raw.trim();
    let lower = v.to_ascii_lowercase();
    match lower.as_str() {
        "fill" => Length::Fill,
        "shrink" => Length::Shrink,
        _ => {
            // `fill 2` / `fill-2` → FillPortion(2).
            if let Some(rest) = lower.strip_prefix("fill") {
                let n = rest.trim_start_matches([' ', '-']).trim();
                if let Ok(p) = n.parse::<u16>() {
                    return Length::FillPortion(p.max(1));
                }
            }
            match v.parse::<f32>() {
                Ok(f) => Length::Fixed(f),
                Err(_) => Length::Shrink,
            }
        }
    }
}

/// Helper to parse Padding
fn parse_padding(s: &Option<String>) -> Padding {
    if let Some(p_str) = s {
        let parts: Vec<f32> = p_str
            .split_whitespace()
            .filter_map(|p| p.parse::<f32>().ok())
            .collect();
        match parts.len() {
            1 => Padding::new(parts[0]),
            2 => Padding {
                top: parts[0],
                right: parts[1],
                bottom: parts[0],
                left: parts[1],
            },
            4 => Padding {
                top: parts[0],
                right: parts[1],
                bottom: parts[2],
                left: parts[3],
            },
            _ => Padding::ZERO,
        }
    } else {
        Padding::ZERO
    }
}

/// Helper to parse alignment
fn parse_alignment(s: &Option<String>) -> Option<Alignment> {
    match s.as_deref() {
        Some("start") | Some("Start") | Some("START") => Some(Alignment::Start),
        Some("center") | Some("Center") | Some("CENTER") => Some(Alignment::Center),
        Some("end") | Some("End") | Some("END") => Some(Alignment::End),
        _ => None,
    }
}

/// Helper to parse hex colors like #RRGGBB or #RRGGBBAA
pub fn parse_hex_color(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&s[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&s[4..6], 16).ok()? as f32 / 255.0;
        Some(Color { r, g, b, a: 1.0 })
    } else if s.len() == 8 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&s[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&s[4..6], 16).ok()? as f32 / 255.0;
        let a = u8::from_str_radix(&s[6..8], 16).ok()? as f32 / 255.0;
        Some(Color { r, g, b, a })
    } else {
        None
    }
}

/// Generate Iced widgets recursively from UiNode tree.
/// References to strings are borrowed directly from the AST node with lifetime 'a.
pub fn render_node<'a>(
    node: &'a UiNode,
    context: &'a HashMap<String, String>,
    editors: &'a EditorMap,
) -> Element<'a, EngineMessage> {
    // `hidden: true` (`display: none`) — sai do layout por completo. Os
    // contêineres Row/Column/Form já filtram filhos ocultos (sem `spacing`
    // fantasma); este retorno cobre os demais caminhos (raiz, filho único de
    // Container/Scrollable/MouseArea) com um `Space` de tamanho zero.
    if node.hidden == Some(true) {
        return Space::new().width(Length::Shrink).height(Length::Shrink).into();
    }
    let mut element: Element<'a, EngineMessage> = match &node.kind {
        NodeType::Text { content, size, bold, color } => {
            let mut t = text(content.as_str());
            if let Some(s) = size {
                t = t.size(*s);
            }
            // `bold` and `font` both influence the font; bold wins, else the hint.
            if *bold {
                t = t.font(Font { weight: iced::font::Weight::Bold, ..Default::default() });
            } else if let Some(f) = font_for(&node.font) {
                t = t.font(f);
            }
            if let Some(c_str) = color {
                if let Some(col) = parse_hex_color(c_str) {
                    t = t.color(col);
                }
            }
            if let Some(align) = parse_text_align(&node.text_align) {
                t = t.align_x(align);
            }
            t.width(parse_length(&node.width))
             .height(parse_length(&node.height))
             .into()
        }
        NodeType::Button { text: btn_text, on_click, navigate_to, navigate_back, color } => {
            let mut t = text(btn_text.as_str());
            if let Some(f) = font_for(&node.font) {
                t = t.font(f);
            }
            // `textAlign` aligns the label inside the button. For a full-width
            // button the label must fill the button's width to actually move,
            // so `center`/`end` get `width: fill` on the text.
            if let Some(align) = parse_text_align(&node.text_align) {
                t = t.align_x(align);
                if !matches!(align, iced::alignment::Horizontal::Left) {
                    t = t.width(Length::Fill);
                }
            }
            let mut btn = button(t);
            // Um botão `disabled` não recebe handler algum: sem `on_press`, o
            // próprio iced já reporta `button::Status::Disabled` na closure de
            // estilo abaixo — não há necessidade de rastrear isso à parte.
            let is_disabled = node.disabled.unwrap_or(false);
            if !is_disabled {
                // Navigation takes priority over the generic on_click.
                if *navigate_back {
                    btn = btn.on_press(EngineMessage::NavigateBack);
                } else if let Some(destination) = navigate_to {
                    btn = btn.on_press(EngineMessage::Navigate(destination.clone()));
                } else if let Some(action) = on_click {
                    btn = btn.on_press(EngineMessage::UiClick(action.clone()));
                }
            }

            if let Some(c_str) = color {
                if let Some(col) = parse_hex_color(c_str) {
                    let br_radius = node.border_radius.unwrap_or(0.0);
                    let br_width = node.border_width.unwrap_or(0.0);
                    let br_color = node.border_color.as_ref()
                        .and_then(|c| parse_hex_color(c))
                        .unwrap_or(Color::TRANSPARENT);
                    // Cor do rótulo: `textColor`/`.classe { text-color }`, senão
                    // branco (default histórico). O `color` do botão é o fundo.
                    let label_col = node.text_color.as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or(Color::WHITE);
                    // Overlays por pseudo-estado (`.classe:hover/:active/:disabled { }`),
                    // já resolvidos em `eval.rs`; `None` quando o `.gss` não declara
                    // aquele estado — nesse caso cai no auto-derive histórico
                    // (±10% de luminância) ou, para `disabled`, 50% de alfa.
                    let hover_ov = node.hover_style.as_deref().cloned();
                    let active_ov = node.active_style.as_deref().cloned();
                    let disabled_ov = node.disabled_style.as_deref().cloned();
                    btn = btn.style(move |_theme, status| {
                        use iced::widget::button::Status;
                        let overlay = match status {
                            Status::Hovered => hover_ov.as_ref(),
                            Status::Pressed => active_ov.as_ref(),
                            Status::Disabled => disabled_ov.as_ref(),
                            Status::Active => None,
                        };
                        let bg_color = overlay
                            .and_then(|r| r.background.as_deref())
                            .and_then(parse_hex_color)
                            .unwrap_or_else(|| match status {
                                Status::Hovered => Color {
                                    r: (col.r * 1.1).min(1.0),
                                    g: (col.g * 1.1).min(1.0),
                                    b: (col.b * 1.1).min(1.0),
                                    a: col.a,
                                },
                                Status::Pressed => Color {
                                    r: (col.r * 0.9).min(1.0),
                                    g: (col.g * 0.9).min(1.0),
                                    b: (col.b * 0.9).min(1.0),
                                    a: col.a,
                                },
                                Status::Disabled => Color { a: col.a * 0.5, ..col },
                                Status::Active => col,
                            });
                        let text_color = overlay
                            .and_then(|r| r.text_color.as_deref())
                            .and_then(parse_hex_color)
                            .unwrap_or(label_col);
                        let radius = overlay.and_then(|r| r.border_radius).unwrap_or(br_radius);
                        let width = overlay.and_then(|r| r.border_width).unwrap_or(br_width);
                        let border_color = overlay
                            .and_then(|r| r.border_color.as_deref())
                            .and_then(parse_hex_color)
                            .unwrap_or(br_color);
                        iced::widget::button::Style {
                            background: Some(Background::Color(bg_color)),
                            text_color,
                            border: Border {
                                radius: iced::border::Radius::new(radius),
                                width,
                                color: border_color,
                            },
                            shadow: iced::Shadow::default(),
                            snap: false,
                        }
                    });
                }
            }

            btn.width(parse_length(&node.width))
               .height(parse_length(&node.height))
               .padding(parse_padding(&node.padding))
               .into()
        }
        NodeType::TextInput { placeholder, value_var, on_change, secure } => {
            let current_value = context.get(value_var).map(|s| s.as_str()).unwrap_or("");
            // Sem `disabled`, sem `.on_input(...)`: o próprio iced reporta
            // `text_input::Status::Disabled` (não editável, cursor não pisca)
            // sem o motor precisar rastrear isso à parte — mesmo truque do botão.
            let is_disabled = node.disabled.unwrap_or(false);

            let mut input = text_input(placeholder.as_str(), current_value).secure(*secure);
            if !is_disabled {
                let action_clone = on_change.clone();
                input = input.on_input(move |val| EngineMessage::UiInputChanged {
                    action: action_clone.clone(),
                    value: val,
                });
            }

            // Wired only once hydrated by an enclosing `<Form>` (`form_scope`
            // set) — a stray `formControl` outside any `<Form>` renders as a
            // plain input, same as before this feature existed. Skipped when
            // `disabled` for the same reason as `on_input` above.
            if !is_disabled {
                if let (Some(control), Some(scope), Some(submit_action)) =
                    (&node.form_control, &node.form_scope, &node.form_submit_action)
                {
                    input = input.id(form_input_id(scope, control));
                    let next_focus = node.form_next_focus.as_ref()
                        .map(|next| form_input_id(scope, next));
                    input = input.on_submit(EngineMessage::UiSubmit {
                        action: submit_action.clone(),
                        next_focus,
                    });
                }
            }

            // `iced`'s own default for `text_input` is `Length::Fill` (unlike
            // most other widgets, which default to `Shrink`); only override it
            // when the template actually sets a `width`, so a plain
            // `<TextInput>` with no `width` attribute still renders at a
            // sane, editable size instead of collapsing to `Shrink`.
            if node.width.is_some() {
                input = input.width(parse_length(&node.width));
            }
            // Same story as `width` above: `iced`'s own default padding
            // (`text_input::DEFAULT_PADDING`, 5px all around) is nonzero;
            // only override it when the template sets one explicitly, instead
            // of collapsing to `Padding::ZERO` (text flush against the edges).
            if node.padding.is_some() {
                input = input.padding(parse_padding(&node.padding));
            }

            // Overlays por pseudo-estado (`:hover`/`:focus`/`:disabled`);
            // parte do estilo padrão do tema (`text_input::default`) e
            // sobrescreve só os campos que o `.gss` realmente declarou.
            if node.hover_style.is_some() || node.focus_style.is_some() || node.disabled_style.is_some() {
                let hover_ov = node.hover_style.as_deref().cloned();
                let focus_ov = node.focus_style.as_deref().cloned();
                let disabled_ov = node.disabled_style.as_deref().cloned();
                input = input.style(move |theme, status| {
                    use iced::widget::text_input::Status;
                    let mut style = iced::widget::text_input::default(theme, status);
                    let overlay = match status {
                        Status::Hovered => hover_ov.as_ref(),
                        Status::Focused { .. } => focus_ov.as_ref(),
                        Status::Disabled => disabled_ov.as_ref(),
                        Status::Active => None,
                    };
                    if let Some(r) = overlay {
                        if let Some(bg) = r.background.as_deref().and_then(parse_hex_color) {
                            style.background = Background::Color(bg);
                        }
                        if let Some(bc) = r.border_color.as_deref().and_then(parse_hex_color) {
                            style.border.color = bc;
                        }
                        if let Some(bw) = r.border_width {
                            style.border.width = bw;
                        }
                        if let Some(br) = r.border_radius {
                            style.border.radius = iced::border::Radius::new(br);
                        }
                        if let Some(tc) = r.text_color.as_deref().and_then(parse_hex_color) {
                            style.value = tc;
                        }
                    }
                    style
                });
            }

            let mut elem: Element<'a, EngineMessage> = input.into();

            if node.height.is_some() {
                elem = container(elem)
                    .height(parse_length(&node.height))
                    .align_y(Alignment::Center)
                    .into();
            }
            elem
        }
        NodeType::TextArea { placeholder, value_var, on_change, readonly } => {
            // The engine keeps the `Content` for this binding (created by
            // `sync_editors` before render). If it is somehow missing on a first
            // frame, fall back to a static placeholder rather than panicking.
            match editors.get(value_var) {
                Some(content) => {
                    let binding = value_var.clone();
                    let on_change = on_change.clone();
                    let readonly = *readonly;
                    let mut ed = text_editor(content)
                        .placeholder(placeholder.as_str())
                        .on_action(move |action| EngineMessage::UiEditorAction {
                            binding: binding.clone(),
                            on_change: on_change.clone(),
                            action,
                            readonly,
                        })
                        .padding(parse_padding(&node.padding));
                    if let Some(f) = font_for(&node.font) {
                        ed = ed.font(f);
                    }
                    ed.height(parse_length(&node.height)).into()
                }
                None => text(placeholder.as_str())
                    .width(parse_length(&node.width))
                    .height(parse_length(&node.height))
                    .into(),
            }
        }
        NodeType::Image { source, clip_circle } => {
            let handle = image::Handle::from_path(source.clone());
            let img = image(handle);
            
            let w_len = parse_length(&node.width);
            let h_len = parse_length(&node.height);

            if *clip_circle {
                let w_val = node.width.as_ref().and_then(|s| s.parse::<f32>().ok()).unwrap_or(80.0);
                let h_val = node.height.as_ref().and_then(|s| s.parse::<f32>().ok()).unwrap_or(80.0);
                let radius = w_val.min(h_val) / 2.0;

                let clipped_img = img.width(Length::Fixed(w_val)).height(Length::Fixed(h_val));
                container(clipped_img)
                    .width(Length::Fixed(w_val))
                    .height(Length::Fixed(h_val))
                    .clip(true)
                    .style(move |_theme| {
                        container::Style {
                            border: Border {
                                radius: iced::border::Radius::new(radius),
                                width: 0.0,
                                color: Color::TRANSPARENT,
                            },
                            ..Default::default()
                        }
                    })
                    .into()
            } else {
                img.width(w_len).height(h_len).into()
            }
        }
        NodeType::Svg { source, color } => {
            let handle = svg::Handle::from_path(source.clone());
            let mut s = svg(handle)
                .width(parse_length(&node.width))
                .height(parse_length(&node.height));
            if let Some(col) = color.as_ref().and_then(|c| parse_hex_color(c)) {
                s = s.style(move |_theme, _status| svg::Style { color: Some(col) });
            }
            s.into()
        }
        NodeType::Scrollable { direction } => {
            let child: Element<'a, EngineMessage> = if let Some(first) = node.children.first() {
                render_node(first, context, editors)
            } else {
                column![].into()
            };
            let dir = match direction.to_ascii_lowercase().as_str() {
                "horizontal" | "h" | "x" => scrollable::Direction::Horizontal(scrollable::Scrollbar::new()),
                "both" | "xy" => scrollable::Direction::Both {
                    vertical: scrollable::Scrollbar::new(),
                    horizontal: scrollable::Scrollbar::new(),
                },
                _ => scrollable::Direction::Vertical(scrollable::Scrollbar::new()),
            };
            scrollable(child)
                .direction(dir)
                .width(parse_length(&node.width))
                .height(parse_length(&node.height))
                .into()
        }
        NodeType::Checkbox { label, checked_var, on_toggle } => {
            let checked = context.get(checked_var).map(|s| is_truthy(s)).unwrap_or(false);
            let mut c = checkbox(checked).label(label.as_str());
            // Sem `disabled`, sem `.on_toggle(...)`: o iced já reporta
            // `checkbox::Status::Disabled` sozinho (mesmo truque do botão).
            if !node.disabled.unwrap_or(false) {
                let action = on_toggle.clone();
                c = c.on_toggle(move |v| EngineMessage::UiInputChanged {
                    action: action.clone(),
                    value: v.to_string(),
                });
            }
            if let Some(s) = node.text_align.as_ref().and_then(|_| node.spacing) {
                c = c.spacing(s);
            }
            c.into()
        }
        NodeType::Toggle { label, checked_var, on_toggle } => {
            let checked = context.get(checked_var).map(|s| is_truthy(s)).unwrap_or(false);
            let mut t = toggler(checked);
            if !node.disabled.unwrap_or(false) {
                let action = on_toggle.clone();
                t = t.on_toggle(move |v| EngineMessage::UiInputChanged {
                    action: action.clone(),
                    value: v.to_string(),
                });
            }
            if !label.is_empty() {
                t = t.label(label.as_str());
            }
            t.into()
        }
        NodeType::Select { options, value_var, on_change, placeholder, label_field, value_field, color } => {
            // Options come from a context JSON array (same shape as ForEach).
            let opts: Vec<SelectOption> = context
                .get(options)
                .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
                .and_then(|v| match v {
                    serde_json::Value::Array(a) => Some(a),
                    _ => None,
                })
                .map(|arr| {
                    arr.iter()
                        .map(|item| SelectOption::from_json(item, label_field, value_field))
                        .collect()
                })
                .unwrap_or_default();

            let current = context.get(value_var).map(|s| s.as_str()).unwrap_or("");
            let selected = opts.iter().find(|o| o.value == current).cloned();
            let action = on_change.clone();

            // Style fields resolved from inline attrs / `.gss` class; anything
            // unset falls back to the active theme palette so it stays consistent.
            let bg = background_for(node);
            let br_radius = node.border_radius;
            let br_width = node.border_width;
            let br_color = node.border_color.as_ref().and_then(|c| parse_hex_color(c));
            let txt_color = color.as_ref().and_then(|c| parse_hex_color(c));

            // `Select`/`pick_list` não tem `Status::Disabled` no iced (o handler
            // é obrigatório), então só o overlay de `:hover` faz sentido aqui.
            let hover_ov = node.hover_style.as_deref().cloned();
            let style_fn = move |theme: &iced::Theme, status: pick_list::Status| {
                let pal = theme.extended_palette();
                let mut text_color = txt_color.unwrap_or(pal.background.base.text);
                let mut background = bg.clone().unwrap_or(Background::Color(pal.background.weak.color));
                let mut border = Border {
                    radius: iced::border::Radius::new(br_radius.unwrap_or(4.0)),
                    width: br_width.unwrap_or(1.0),
                    color: br_color.unwrap_or(pal.background.strong.color),
                };
                if matches!(status, pick_list::Status::Hovered | pick_list::Status::Opened { .. }) {
                    border.color = txt_color.unwrap_or(pal.primary.base.color);
                    if let Some(r) = hover_ov.as_ref() {
                        if let Some(bg2) = r.background.as_deref().and_then(parse_hex_color) {
                            background = Background::Color(bg2);
                        }
                        if let Some(bc) = r.border_color.as_deref().and_then(parse_hex_color) {
                            border.color = bc;
                        }
                        if let Some(bw) = r.border_width {
                            border.width = bw;
                        }
                        if let Some(br2) = r.border_radius {
                            border.radius = iced::border::Radius::new(br2);
                        }
                        if let Some(tc) = r.text_color.as_deref().and_then(parse_hex_color) {
                            text_color = tc;
                        }
                    }
                }
                pick_list::Style {
                    text_color,
                    placeholder_color: pal.background.strong.color,
                    handle_color: text_color,
                    background,
                    border,
                }
            };

            let mut pl = pick_list(opts, selected, move |chosen: SelectOption| {
                EngineMessage::UiInputChanged { action: action.clone(), value: chosen.value }
            })
            .style(style_fn)
            .width(parse_length(&node.width))
            .padding(parse_padding(&node.padding));

            if !placeholder.is_empty() {
                pl = pl.placeholder(placeholder.clone());
            }
            if let Some(f) = font_for(&node.font) {
                pl = pl.font(f);
            }

            let mut elem: Element<'a, EngineMessage> = pl.into();
            if node.height.is_some() {
                elem = container(elem)
                    .height(parse_length(&node.height))
                    .align_y(Alignment::Center)
                    .into();
            }
            elem
        }
        NodeType::Rule { horizontal } => {
            // Thickness comes from the cross dimension; default 1px.
            if *horizontal {
                let h = node.height.as_ref().and_then(|s| s.parse::<f32>().ok()).unwrap_or(1.0);
                rule::horizontal(h).into()
            } else {
                let w = node.width.as_ref().and_then(|s| s.parse::<f32>().ok()).unwrap_or(1.0);
                rule::vertical(w).into()
            }
        }
        NodeType::Column => {
            let mut col = column![];
            
            if let Some(align_val) = parse_alignment(&node.align_x) {
                col = col.align_x(align_val);
            }
            
            if let Some(sp) = node.spacing {
                col = col.spacing(sp);
            }
            
            col = col.padding(parse_padding(&node.padding));

            for child in node.children.iter().filter(|c| c.hidden != Some(true)) {
                col = col.push(render_node(child, context, editors));
            }
            
            col.width(parse_length(&node.width))
               .height(parse_length(&node.height))
               .into()
        }
        NodeType::Row => {
            let mut r = row![];
            
            if let Some(align_val) = parse_alignment(&node.align_y) {
                r = r.align_y(align_val);
            }
            
            if let Some(sp) = node.spacing {
                r = r.spacing(sp);
            }
            
            r = r.padding(parse_padding(&node.padding));

            for child in node.children.iter().filter(|c| c.hidden != Some(true)) {
                r = r.push(render_node(child, context, editors));
            }
            
            r.width(parse_length(&node.width))
             .height(parse_length(&node.height))
             .into()
        }
        NodeType::Form { .. } => {
            // A `<Form>` is a layout container like `<Column>` — its
            // `onSubmit`/`formControl` wiring lives entirely in the hydrated
            // `form_*` fields of its descendants (see `eval.rs`), nothing to
            // render here beyond stacking its children.
            let mut col = column![];

            if let Some(align_val) = parse_alignment(&node.align_x) {
                col = col.align_x(align_val);
            }
            if let Some(sp) = node.spacing {
                col = col.spacing(sp);
            }
            col = col.padding(parse_padding(&node.padding));

            for child in node.children.iter().filter(|c| c.hidden != Some(true)) {
                col = col.push(render_node(child, context, editors));
            }

            col.width(parse_length(&node.width))
               .height(parse_length(&node.height))
               .into()
        }
        NodeType::Container => {
            let child: Element<'a, EngineMessage> = if let Some(first_child) = node.children.first() {
                render_node(first_child, context, editors)
            } else {
                column![].into()
            };

            let mut c = container(child);
            c = c.width(parse_length(&node.width))
                 .height(parse_length(&node.height))
                 .padding(parse_padding(&node.padding));

            if let Some(ax) = parse_alignment(&node.align_x) {
                c = c.align_x(ax);
            }
            if let Some(ay) = parse_alignment(&node.align_y) {
                c = c.align_y(ay);
            }

            let bg_opt = background_for(node);
            let br_opt = node.border_radius;
            let bw_opt = node.border_width.unwrap_or(0.0);
            let bc_opt = node.border_color.as_ref().and_then(|bc| parse_hex_color(bc));

            if bg_opt.is_some() || br_opt.is_some() || bw_opt > 0.0 {
                c = c.style(move |_theme| {
                    container::Style {
                        background: bg_opt.clone(),
                        border: Border {
                            radius: iced::border::Radius::new(br_opt.unwrap_or(0.0)),
                            width: bw_opt,
                            color: bc_opt.unwrap_or(Color::TRANSPARENT),
                        },
                        ..Default::default()
                    }
                });
            }

            c.into()
        }
        NodeType::Include { .. } => {
            container(text("Unresolved Include").color(Color::from_rgb(1.0, 0.0, 0.0))).into()
        }
        NodeType::Component { name, .. } => {
            container(text(format!("Unresolved component <{}>", name)).color(Color::from_rgb(1.0, 0.0, 0.0))).into()
        }
        NodeType::ForEach { .. } => {
            // TODO(diretivas): forma legada por tag; preferir atributos if/else/for-each. Remover quando templates forem migrados.
            // ForEach is expanded during evaluation; nothing to render directly.
            column![].into()
        }
        NodeType::Import { .. } => {
            // Import declarations are stripped during evaluation; render nothing.
            column![].into()
        }
        NodeType::Link { .. } => {
            // <link> declarations are stripped during evaluation; render nothing.
            column![].into()
        }
        NodeType::Style { .. } => {
            // Inline <style> blocks are stripped during evaluation; render nothing.
            column![].into()
        }
        NodeType::If { .. } | NodeType::Else => {
            // TODO(diretivas): forma legada por tag; preferir atributos if/else/for-each. Remover quando templates forem migrados.
            // if/else are expanded during evaluation; nothing to render directly.
            column![].into()
        }
        NodeType::Fragment => {
            // A `Fragment`'s children are normally spliced into the parent
            // during evaluation (`expand_children`), so it seldom reaches
            // rendering; when it does (e.g. a multi-root screen root), stack
            // its children in a plain `Column`.
            let mut col = column![];
            if let Some(sp) = node.spacing {
                col = col.spacing(sp);
            }
            for child in node.children.iter().filter(|c| c.hidden != Some(true)) {
                col = col.push(render_node(child, context, editors));
            }
            col.width(parse_length(&node.width))
               .height(parse_length(&node.height))
               .into()
        }
    };

    // Wrap elements other than Container in a Container if a background/gradient
    // or borders are specified.
    if node.kind != NodeType::Container {
        let bg_opt = background_for(node);
        let br_opt = node.border_radius;
        let bw_opt = node.border_width.unwrap_or(0.0);
        let bc_opt = node.border_color.as_ref().and_then(|bc| parse_hex_color(bc));

        if bg_opt.is_some() || br_opt.is_some() || bw_opt > 0.0 {
            let mut c = container(element);
            c = c.width(parse_length(&node.width))
                 .height(parse_length(&node.height));

            if let Some(ax) = parse_alignment(&node.align_x) {
                c = c.align_x(ax);
            }
            if let Some(ay) = parse_alignment(&node.align_y) {
                c = c.align_y(ay);
            }

            c = c.style(move |_theme| {
                container::Style {
                    background: bg_opt.clone(),
                    border: Border {
                        radius: iced::border::Radius::new(br_opt.unwrap_or(0.0)),
                        width: bw_opt,
                        color: bc_opt.unwrap_or(Color::TRANSPARENT),
                    },
                    ..Default::default()
                }
            });
            element = c.into();
        }
    }

    // `maxWidth`/`maxHeight`: `Row`/`Column` do iced não capam o próprio
    // tamanho, então qualquer nó que declare um teto é envolto num `container`
    // que limita (inerte fora isso). Antes do `mouse_area`, para a superfície
    // interativa cobrir o elemento já restrito.
    if node.max_width.is_some() || node.max_height.is_some() {
        let mut c = container(element);
        if let Some(mw) = node.max_width {
            c = c.max_width(mw);
        }
        if let Some(mh) = node.max_height {
            c = c.max_height(mh);
        }
        element = c.into();
    }

    // A node with `on_press`, `on_double_click` and/or `cursor` is wrapped in a
    // `mouse_area`: `on_press` fires on mouse-button-down (not release like a
    // `Button`) — needed for window drag/resize (`onPress="window:drag"`);
    // `on_double_click` covers e.g. titlebar double-click to maximize; and
    // `cursor` sets the hover pointer (resize arrows on edge handles). Applied
    // last so the whole styled element is the interactive surface.
    if node.on_press.is_some() || node.on_double_click.is_some() || node.cursor.is_some()
        || node.drag_item_key.is_some()
    {
        let mut ma = mouse_area(element);
        if let Some(action) = &node.on_press {
            ma = ma.on_press(EngineMessage::UiClick(action.clone()));
        }
        if let Some(action) = &node.on_double_click {
            ma = ma.on_double_click(EngineMessage::UiClick(action.clone()));
        }
        if let Some(interaction) = node.cursor.as_deref().and_then(cursor_interaction) {
            ma = ma.interaction(interaction);
        }
        // Drag-and-drop reordering (see `UiNode::drag_*`, hydrated by the
        // for-each expansion of a reorderable list in `eval.rs`): every item of
        // such a list is a valid drop/hover target; only its `dragHandle`
        // descendant also starts the drag on press.
        if let (Some(list), Some(key)) = (&node.drag_list, &node.drag_item_key) {
            ma = ma.on_enter(EngineMessage::DragHover { list: list.clone(), key: key.clone() });
        }
        if node.drag_handle {
            if let (Some(list), Some(key), Some(order), Some(on_reorder), Some(reorder_key)) = (
                &node.drag_list,
                &node.drag_item_key,
                &node.drag_order,
                &node.drag_on_reorder,
                &node.drag_reorder_key,
            ) {
                ma = ma.on_press(EngineMessage::DragStart {
                    list: list.clone(),
                    reorder_key: reorder_key.clone(),
                    on_reorder: on_reorder.clone(),
                    order: order.clone(),
                    key: key.clone(),
                });
            }
        }
        element = ma.into();
    }

    // `tooltip="..."` — envolve por último, DEPOIS do mouse_area acima, para o
    // balão reagir ao hover sobre a superfície interativa inteira (não só o
    // conteúdo visual). Sem `.style()` explícito: `container::dark` é um
    // helper embutido do iced, independente do tema ativo (fundo quase preto +
    // texto branco) — não precisa de fiação nova com o `theme.json` do app
    // pra ficar legível em qualquer paleta.
    if let Some(tip) = node.tooltip.as_deref().filter(|s| !s.is_empty()) {
        let position = match node.tooltip_position.as_deref() {
            Some("bottom") => TooltipPosition::Bottom,
            Some("left") => TooltipPosition::Left,
            Some("follow") | Some("follow_cursor") | Some("cursor") => {
                TooltipPosition::FollowCursor
            }
            Some("top") => TooltipPosition::Top,
            _ => TooltipPosition::Right,
        };
        let label = container(text(tip.to_string()).size(12))
            .padding([4, 8])
            .style(container::dark);
        element = Tooltip::new(element, label, position).gap(6).into();
    }

    element
}

/// Maps a `cursor="…"` value to an [`iced::mouse::Interaction`]. Covers the
/// common pointers plus the window-resize arrows used by borderless titlebars.
/// Returns `None` for unknown names (the default cursor is kept).
fn cursor_interaction(name: &str) -> Option<iced::mouse::Interaction> {
    // Not glob-importing the variants: `Interaction::None` would shadow
    // `Option::None` below.
    use iced::mouse::Interaction::{
        Pointer, Text, Grab, Grabbing, Move, Crosshair, Wait, Progress, Help,
        NotAllowed, Hidden, ResizingHorizontally, ResizingVertically,
        ResizingDiagonallyUp, ResizingDiagonallyDown,
    };
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "pointer" | "hand" => Pointer,
        "text" => Text,
        "grab" => Grab,
        "grabbing" => Grabbing,
        "move" | "all-scroll" => Move,
        "crosshair" => Crosshair,
        "wait" => Wait,
        "progress" => Progress,
        "help" => Help,
        "not-allowed" | "no-drop" => NotAllowed,
        "none" | "hidden" => Hidden,
        // Window-resize handles (compass + axis aliases).
        "resize-h" | "ew" | "ew-resize" | "e" | "w" | "col-resize" => ResizingHorizontally,
        "resize-v" | "ns" | "ns-resize" | "n" | "s" | "row-resize" => ResizingVertically,
        "resize-ne" | "ne" | "sw" | "nesw" | "nesw-resize" => ResizingDiagonallyUp,
        "resize-nw" | "nw" | "se" | "nwse" | "nwse-resize" => ResizingDiagonallyDown,
        _ => return None,
    })
}

#[cfg(test)]
mod length_tests {
    use super::parse_length;
    use iced::Length;

    fn len(s: &str) -> Length {
        parse_length(&Some(s.to_string()))
    }

    #[test]
    fn keywords_and_case() {
        assert_eq!(len("fill"), Length::Fill);
        assert_eq!(len("FILL"), Length::Fill);
        assert_eq!(len("Shrink"), Length::Shrink);
        assert_eq!(parse_length(&None), Length::Shrink);
    }

    #[test]
    fn fixed_px() {
        assert_eq!(len("640"), Length::Fixed(640.0));
        assert_eq!(len("  34 "), Length::Fixed(34.0));
    }

    #[test]
    fn fill_portion_weights() {
        assert_eq!(len("fill 2"), Length::FillPortion(2));
        assert_eq!(len("fill-3"), Length::FillPortion(3));
        assert_eq!(len("FILL 4"), Length::FillPortion(4));
        // `fill 0` não faz sentido; normaliza para 1 (== Fill).
        assert_eq!(len("fill 0"), Length::FillPortion(1));
    }

    #[test]
    fn garbage_falls_back_to_shrink() {
        assert_eq!(len("wibble"), Length::Shrink);
    }
}
