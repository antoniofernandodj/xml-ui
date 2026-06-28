use std::collections::HashMap;
use iced::widget::{
    button, column, row, text, container, text_input, image, svg, scrollable,
    checkbox, toggler, horizontal_rule, vertical_rule,
};
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
    XmlClick(String),
    XmlInputChanged { action: String, value: String },
    /// Navigate to the given screen (button with `navigateTo`).
    Navigate(String),
    /// Go back to the previous screen (button with `navigateBack`).
    NavigateBack,
    FileChanged(String),
    /// Merge `(key, value)` pairs into the context and re-evaluate. Produced by
    /// async effects ([`crate::component::Effect`]) completing and by component
    /// subscriptions; the host app just forwards it to [`crate::GlacierUI::dispatch`].
    ContextPatch(Vec<(String, String)>),
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
        Some(Color::new(r, g, b, 1.0))
    } else if s.len() == 8 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&s[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&s[4..6], 16).ok()? as f32 / 255.0;
        let a = u8::from_str_radix(&s[6..8], 16).ok()? as f32 / 255.0;
        Some(Color::new(r, g, b, a))
    } else {
        None
    }
}

/// Generate Iced widgets recursively from UiNode tree.
/// References to strings are borrowed directly from the AST node with lifetime 'a.
pub fn render_node<'a>(
    node: &'a UiNode,
    context: &'a HashMap<String, String>,
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
                btn = btn.on_press(EngineMessage::XmlClick(action.clone()));
            }
            
            if let Some(c_str) = color {
                if let Some(col) = parse_hex_color(c_str) {
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
                            border: Border::default(),
                            shadow: iced::Shadow::default(),
                        }
                    });
                }
            }
            
            btn.width(parse_length(&node.width))
               .height(parse_length(&node.height))
               .padding(parse_padding(&node.padding))
               .into()
        }
        NodeType::TextInput { placeholder, value_var, on_change } => {
            let current_value = context.get(value_var).map(|s| s.as_str()).unwrap_or("");
            let action_clone = on_change.clone();
            
            let mut input = text_input(placeholder.as_str(), current_value)
                .on_input(move |val| EngineMessage::XmlInputChanged {
                    action: action_clone.clone(),
                    value: val,
                });
            
            input = input.width(parse_length(&node.width))
                         .padding(parse_padding(&node.padding));

            let mut elem: Element<'a, EngineMessage> = input.into();
            
            if node.height.is_some() {
                elem = container(elem)
                    .height(parse_length(&node.height))
                    .align_y(Alignment::Center)
                    .into();
            }
            elem
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
                render_node(first, context)
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
            let mut c = checkbox(label.as_str(), checked)
                .on_toggle(move |v| EngineMessage::XmlInputChanged {
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
            let mut t = toggler(checked).on_toggle(move |v| EngineMessage::XmlInputChanged {
                action: action.clone(),
                value: v.to_string(),
            });
            if !label.is_empty() {
                t = t.label(label.as_str());
            }
            t.into()
        }
        NodeType::Rule { horizontal } => {
            // Thickness comes from the cross dimension; default 1px.
            if *horizontal {
                let h = node.height.as_ref().and_then(|s| s.parse::<u16>().ok()).unwrap_or(1);
                horizontal_rule(h).into()
            } else {
                let w = node.width.as_ref().and_then(|s| s.parse::<u16>().ok()).unwrap_or(1);
                vertical_rule(w).into()
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
                col = col.push(render_node(child, context));
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
                r = r.push(render_node(child, context));
            }
            
            r.width(parse_length(&node.width))
             .height(parse_length(&node.height))
             .into()
        }
        NodeType::Container => {
            let child: Element<'a, EngineMessage> = if let Some(first_child) = node.children.first() {
                render_node(first_child, context)
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
        NodeType::If { .. } | NodeType::Else => {
            // if/else are expanded during evaluation; nothing to render directly.
            column![].into()
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

    element
}
