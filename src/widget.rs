use std::collections::HashMap;
use iced::widget::{
    button, column, row, text, container, text_input, text_editor, image, svg, scrollable,
    checkbox, toggler, rule, pick_list, mouse_area,
};

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
    UiEditorAction { binding: String, on_change: String, action: text_editor::Action },
    /// Navigate to the given screen (button with `navigateTo`).
    Navigate(String),
    /// Go back to the previous screen (button with `navigateBack`).
    NavigateBack,
    FileChanged(String),
    /// Merge `(key, value)` pairs into the context and re-evaluate. Produced by
    /// async effects ([`crate::component::Effect`]) completing and by component
    /// subscriptions; the host app just forwards it to [`crate::GlacierUI::dispatch`].
    ContextPatch(Vec<(String, String)>),
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
}

/// The stable focus id of a form-bound `TextInput`: `scope` is the enclosing
/// `<Form>`'s `"{owner}::{form name}"` prefix (shared by every control in that
/// form), `control` its own `formControl` name.
pub fn form_input_id(scope: &str, control: &str) -> String {
    format!("glacier_form::{scope}::{control}")
}

/// Helper to parse iced::Length from optional string
fn parse_length(s: &Option<String>) -> Length {
    match s.as_deref() {
        Some("fill") | Some("Fill") | Some("FILL") => Length::Fill,
        Some("shrink") | Some("Shrink") | Some("SHRINK") => Length::Shrink,
        Some(val) => {
            if let Ok(f) = val.parse::<f32>() {
                Length::Fixed(f)
            } else {
                Length::Shrink
            }
        }
        None => Length::Shrink,
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
            // Navigation takes priority over the generic on_click.
            if *navigate_back {
                btn = btn.on_press(EngineMessage::NavigateBack);
            } else if let Some(destination) = navigate_to {
                btn = btn.on_press(EngineMessage::Navigate(destination.clone()));
            } else if let Some(action) = on_click {
                btn = btn.on_press(EngineMessage::UiClick(action.clone()));
            }
            
            if let Some(c_str) = color {
                if let Some(col) = parse_hex_color(c_str) {
                    let br_radius = node.border_radius.unwrap_or(0.0);
                    let br_width = node.border_width.unwrap_or(0.0);
                    let br_color = node.border_color.as_ref()
                        .and_then(|c| parse_hex_color(c))
                        .unwrap_or(Color::TRANSPARENT);
                    btn = btn.style(move |_theme, status| {
                        let bg = match status {
                            iced::widget::button::Status::Hovered => Some(Background::Color(Color {
                                r: (col.r * 1.1).min(1.0),
                                g: (col.g * 1.1).min(1.0),
                                b: (col.b * 1.1).min(1.0),
                                a: col.a,
                            })),
                            iced::widget::button::Status::Pressed => Some(Background::Color(Color {
                                r: (col.r * 0.9).min(1.0),
                                g: (col.g * 0.9).min(1.0),
                                b: (col.b * 0.9).min(1.0),
                                a: col.a,
                            })),
                            _ => Some(Background::Color(col)),
                        };
                        iced::widget::button::Style {
                            background: bg,
                            text_color: Color::WHITE,
                            border: Border {
                                radius: iced::border::Radius::new(br_radius),
                                width: br_width,
                                color: br_color,
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
            let action_clone = on_change.clone();

            let mut input = text_input(placeholder.as_str(), current_value)
                .on_input(move |val| EngineMessage::UiInputChanged {
                    action: action_clone.clone(),
                    value: val,
                })
                .secure(*secure);

            // Wired only once hydrated by an enclosing `<Form>` (`form_scope`
            // set) — a stray `formControl` outside any `<Form>` renders as a
            // plain input, same as before this feature existed.
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

            let mut elem: Element<'a, EngineMessage> = input.into();

            if node.height.is_some() {
                elem = container(elem)
                    .height(parse_length(&node.height))
                    .align_y(Alignment::Center)
                    .into();
            }
            elem
        }
        NodeType::TextArea { placeholder, value_var, on_change } => {
            // The engine keeps the `Content` for this binding (created by
            // `sync_editors` before render). If it is somehow missing on a first
            // frame, fall back to a static placeholder rather than panicking.
            match editors.get(value_var) {
                Some(content) => {
                    let binding = value_var.clone();
                    let on_change = on_change.clone();
                    let mut ed = text_editor(content)
                        .placeholder(placeholder.as_str())
                        .on_action(move |action| EngineMessage::UiEditorAction {
                            binding: binding.clone(),
                            on_change: on_change.clone(),
                            action,
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
            let action = on_toggle.clone();
            let mut c = checkbox(checked)
                .label(label.as_str())
                .on_toggle(move |v| EngineMessage::UiInputChanged {
                    action: action.clone(),
                    value: v.to_string(),
                });
            if let Some(s) = node.text_align.as_ref().and_then(|_| node.spacing) {
                c = c.spacing(s);
            }
            c.into()
        }
        NodeType::Toggle { label, checked_var, on_toggle } => {
            let checked = context.get(checked_var).map(|s| is_truthy(s)).unwrap_or(false);
            let action = on_toggle.clone();
            let mut t = toggler(checked).on_toggle(move |v| EngineMessage::UiInputChanged {
                action: action.clone(),
                value: v.to_string(),
            });
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

            let style_fn = move |theme: &iced::Theme, status: pick_list::Status| {
                let pal = theme.extended_palette();
                let text_color = txt_color.unwrap_or(pal.background.base.text);
                let mut border = Border {
                    radius: iced::border::Radius::new(br_radius.unwrap_or(4.0)),
                    width: br_width.unwrap_or(1.0),
                    color: br_color.unwrap_or(pal.background.strong.color),
                };
                if matches!(status, pick_list::Status::Hovered | pick_list::Status::Opened { .. }) {
                    border.color = txt_color.unwrap_or(pal.primary.base.color);
                }
                pick_list::Style {
                    text_color,
                    placeholder_color: pal.background.strong.color,
                    handle_color: text_color,
                    background: bg.clone().unwrap_or(Background::Color(pal.background.weak.color)),
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

            for child in &node.children {
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

            for child in &node.children {
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

            for child in &node.children {
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
            for child in &node.children {
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
