use std::collections::HashMap;
use crate::parser::{UiNode, NodeType};
use crate::stylesheet::{StyleSheet, StyleRule, resolve_classes};

/// Splits a `<script>...</script>` block out of an XML document, returning the
/// markup with the block removed and the script body (if any).
///
/// The script is stripped *before* XML parsing, so it may sit as a sibling of
/// the root element (it would otherwise make the document multi-rooted). The
/// engine ignores the script at runtime; it is consumed at compile time by the
/// `#[component]` macro to generate behavior.
pub fn strip_script(xml: &str) -> (String, Option<String>) {
    let lower = xml.to_ascii_lowercase();
    if let Some(open_start) = lower.find("<script") {
        // Find the end of the opening tag (supports `<script>` and `<script ...>`).
        if let Some(gt_rel) = lower[open_start..].find('>') {
            let body_start = open_start + gt_rel + 1;
            if let Some(close_rel) = lower[body_start..].find("</script>") {
                let body_end = body_start + close_rel;
                let close_end = body_end + "</script>".len();
                let script = xml[body_start..body_end].to_string();
                let mut markup = String::with_capacity(xml.len());
                markup.push_str(&xml[..open_start]);
                markup.push_str(&xml[close_end..]);
                return (markup, Some(script));
            }
        }
    }
    (xml.to_string(), None)
}

/// Normalizes bare directives like `else` or `senao` (without value) inside XML tags
/// by rewriting them to `else=""` or `senao=""` before XML parsing.
pub fn normalize_bare_directives(xml: &str) -> String {
    let mut result = String::with_capacity(xml.len());
    let mut in_tag = false;
    let mut in_comment = false;
    let mut quote_char = None;
    let chars: Vec<char> = xml.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if in_comment {
            // Check for end of comment "-->"
            if i + 2 < chars.len() && chars[i] == '-' && chars[i+1] == '-' && chars[i+2] == '>' {
                result.push('-');
                result.push('-');
                result.push('>');
                in_comment = false;
                i += 3;
            } else {
                result.push(chars[i]);
                i += 1;
            }
            continue;
        }

        // Check for start of comment "<!--"
        if i + 3 < chars.len() && chars[i] == '<' && chars[i+1] == '!' && chars[i+2] == '-' && chars[i+3] == '-' {
            result.push_str("<!--");
            in_comment = true;
            i += 4;
            continue;
        }

        let c = chars[i];
        if !in_tag {
            if c == '<' {
                in_tag = true;
                quote_char = None;
            }
            result.push(c);
            i += 1;
        } else {
            // We are inside a tag
            if c == '>' {
                in_tag = false;
                result.push(c);
                i += 1;
            } else if let Some(q) = quote_char {
                if c == q {
                    quote_char = None;
                }
                result.push(c);
                i += 1;
            } else {
                // Not in quotes
                if c == '"' || c == '\'' {
                    quote_char = Some(c);
                    result.push(c);
                    i += 1;
                } else {
                    // Check for else or senao
                    let mut matched_len = None;
                    let mut replaced_with = None;

                    // Match "else" or "senao" (case-insensitive)
                    let remaining_len = chars.len() - i;
                    if remaining_len >= 4 {
                        let word: String = chars[i..i+4].iter().collect();
                        if word.eq_ignore_ascii_case("else") {
                            matched_len = Some(4);
                            replaced_with = Some("else=\"\"");
                        }
                    }
                    if matched_len.is_none() && remaining_len >= 5 {
                        let word: String = chars[i..i+5].iter().collect();
                        if word.eq_ignore_ascii_case("senao") {
                            matched_len = Some(5);
                            replaced_with = Some("senao=\"\"");
                        }
                    }

                    if let (Some(len), Some(replacement)) = (matched_len, replaced_with) {
                        // Check preceding character (must be whitespace for an attribute)
                        let preceded_ok = i > 0 && chars[i - 1].is_ascii_whitespace();

                        if preceded_ok {
                            // Check succeeding characters to see if it's followed by '='
                            let mut next_idx = i + len;
                            while next_idx < chars.len() && chars[next_idx].is_ascii_whitespace() {
                                next_idx += 1;
                            }
                            let is_followed_by_equals = next_idx < chars.len() && chars[next_idx] == '=';

                            if !is_followed_by_equals {
                                // It is a bare attribute! Replace it.
                                result.push_str(replacement);
                                i += len;
                                continue;
                            }
                        }
                    }

                    result.push(c);
                    i += 1;
                }
            }
        }
    }
    result
}

/// Process string template by replacing `{key}` placeholders with values from context
pub fn process_template(template: &str, context: &HashMap<String, String>) -> String {
    let mut result = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut key = String::new();
            let mut closed = false;
            while let Some(&nc) = chars.peek() {
                if nc == '}' {
                    chars.next(); // Consume '}'
                    closed = true;
                    break;
                } else {
                    key.push(chars.next().unwrap());
                }
            }
            if closed {
                if let Some(val) = context.get(&key) {
                    result.push_str(val);
                } else {
                    // Placeholder key not found; we leave it as is or empty. Let's make it empty.
                }
            } else {
                result.push('{');
                result.push_str(&key);
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Whether a (already-interpolated) string should be considered true.
fn is_truthy(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "yes" | "on" | "sim"
    )
}

/// Evaluate an `<if>` condition against the context.
/// With `equals`/`not_equals` it compares strings; otherwise it is a truthy check.
fn eval_condition(
    cond: &str,
    equals: &Option<String>,
    not_equals: &Option<String>,
    context: &HashMap<String, String>,
) -> bool {
    let value = process_template(cond, context);
    if let Some(eq) = equals {
        return value == process_template(eq, context);
    }
    if let Some(ne) = not_equals {
        return value != process_template(ne, context);
    }
    is_truthy(&value)
}

/// The stylesheets in effect during evaluation, split by scope.
///
/// `global` sheets (loaded via `GlacierUI::load_stylesheet`) apply everywhere.
/// `by_component` holds the sheets a component declared with
/// `<link rel="stylesheet">`, keyed by component name; they apply only inside
/// that component's subtree, layered *on top of* the global ones so a scoped
/// class can override a global one locally.
pub struct StyleContext<'a> {
    pub global: &'a [StyleSheet],
    pub by_component: &'a HashMap<String, Vec<StyleSheet>>,
}

impl<'a> StyleContext<'a> {
    /// The ordered sheets that apply for the given component scope: global
    /// first (lowest priority), then that component's own scoped sheets.
    fn active(&self, scope: Option<&str>) -> Vec<&StyleSheet> {
        let mut sheets: Vec<&StyleSheet> = self.global.iter().collect();
        if let Some(name) = scope {
            if let Some(scoped) = self.by_component.get(name) {
                sheets.extend(scoped.iter());
            }
        }
        sheets
    }
}

/// Expands a sibling list of children into evaluated nodes, applying the
/// structural rules: `<if>`/`<else>` are resolved against the context (binding
/// `<else>` to the immediately preceding `<if>`), `<ForEach>` is unrolled over
/// its JSON array (re-expanding its own body so nested `if`/`else`/`ForEach`
/// work at any depth), and `<import>`/`<link>` are dropped. Everything else is
/// evaluated normally and pushed to `out`.
#[allow(clippy::too_many_arguments)]
fn expand_children(
    children: &[UiNode],
    context: &HashMap<String, String>,
    templates: &HashMap<String, UiNode>,
    styles: &StyleContext,
    scope: Option<&str>,
    owner: Option<&str>,
    out: &mut Vec<UiNode>,
) -> Result<(), String> {
    // Tracks the result of the immediately preceding `<if>`, so an `<else>`
    // can bind to it. Reset by any other (non-else) node.
    let mut last_if: Option<bool> = None;
    for child in children {
        if matches!(child.kind, NodeType::Import { .. } | NodeType::Link { .. }) {
            continue;
        }

        // 1. Process for-each attribute directive (outer precedence)
        if let Some(items) = &child.for_each {
            let var = child.for_each_var.as_deref().unwrap_or("item");
            let items_evaluated = process_template(items, context);
            if let Some(json_str) = context.get(&items_evaluated) {
                if let Ok(serde_json::Value::Array(arr)) =
                    serde_json::from_str::<serde_json::Value>(json_str)
                {
                    for item in arr {
                        let mut local_context = context.clone();
                        match item {
                            serde_json::Value::Object(obj) => {
                                for (key, val) in obj {
                                    let str_val = match val {
                                        serde_json::Value::String(s) => s,
                                        other => other.to_string(),
                                    };
                                    local_context.insert(format!("{}.{}", var, key), str_val);
                                }
                            }
                            serde_json::Value::String(s) => {
                                local_context.insert(var.to_string(), s);
                            }
                            other => {
                                local_context.insert(var.to_string(), other.to_string());
                            }
                        }
                        // Clone the child without the for_each directive
                        let mut clone = child.clone();
                        clone.for_each = None;
                        clone.for_each_var = None;

                        // Expand the single child in the new context (which will evaluate its if condition if present)
                        expand_children(
                            std::slice::from_ref(&clone),
                            &local_context,
                            templates,
                            styles,
                            scope,
                            owner,
                            out,
                        )?;
                    }
                }
            }
            last_if = None;
            continue;
        }

        // 2. Process else attribute directive
        if child.is_else {
            if last_if == Some(false) {
                // Clone child and clear else directive
                let mut clone = child.clone();
                clone.is_else = false;
                out.push(eval_owned(&clone, context, templates, styles, scope, owner)?);
            }
            last_if = None;
            continue;
        }

        // 3. Process if attribute directive
        if let Some(cond) = &child.if_cond {
            let truthy = eval_condition(cond, &child.if_equals, &child.if_not_equals, context);
            if truthy {
                // Clone child and clear if directives
                let mut clone = child.clone();
                clone.if_cond = None;
                clone.if_equals = None;
                clone.if_not_equals = None;
                out.push(eval_owned(&clone, context, templates, styles, scope, owner)?);
            }
            last_if = Some(truthy);
            continue;
        }

        // 4. Fallback to legacy tag-based conditionals/loops
        match &child.kind {
            // `<import>`/`<link>` declarations are skipped above.
            NodeType::Import { .. } | NodeType::Link { .. } => {}
            NodeType::ForEach { items, var } => {
                let items_evaluated = process_template(items, context);
                if let Some(json_str) = context.get(&items_evaluated) {
                    if let Ok(serde_json::Value::Array(arr)) =
                        serde_json::from_str::<serde_json::Value>(json_str)
                    {
                        for item in arr {
                            let mut local_context = context.clone();
                            match item {
                                serde_json::Value::Object(obj) => {
                                    for (key, val) in obj {
                                        let str_val = match val {
                                            serde_json::Value::String(s) => s,
                                            other => other.to_string(),
                                        };
                                        local_context.insert(format!("{}.{}", var, key), str_val);
                                    }
                                }
                                serde_json::Value::String(s) => {
                                    local_context.insert(var.clone(), s);
                                }
                                other => {
                                    local_context.insert(var.clone(), other.to_string());
                                }
                            }
                            // Re-run the structural expansion on the body so that
                            // nested `if`/`else`/`ForEach` are honoured per item.
                            expand_children(
                                &child.children,
                                &local_context,
                                templates,
                                styles,
                                scope,
                                owner,
                                out,
                            )?;
                        }
                    }
                }
                last_if = None;
            }
            NodeType::If { cond, equals, not_equals } => {
                let truthy = eval_condition(cond, equals, not_equals, context);
                if truthy {
                    expand_children(&child.children, context, templates, styles, scope, owner, out)?;
                }
                last_if = Some(truthy);
            }
            NodeType::Else => {
                if last_if == Some(false) {
                    expand_children(&child.children, context, templates, styles, scope, owner, out)?;
                }
                last_if = None;
            }
            _ => {
                out.push(eval_owned(child, context, templates, styles, scope, owner)?);
                last_if = None;
            }
        }
    }
    Ok(())
}

/// Recursively evaluate a UiNode tree, resolving templates and placeholders.
///
/// `styles` are the loaded `.gss` documents; any `class="..."` on a node is
/// resolved against them and merged underneath the node's inline attributes.
/// `scope` is the name of the component being evaluated, used to pick up its
/// `<link>`-scoped stylesheets.
pub fn evaluate_node(
    node: &UiNode,
    context: &HashMap<String, String>,
    templates: &HashMap<String, UiNode>,
    styles: &StyleContext,
    scope: Option<&str>,
) -> Result<UiNode, String> {
    eval_owned(node, context, templates, styles, scope, None)
}

/// Prefixes an action with its owning component, so `dispatch` can route it.
/// Actions inside a `<Component name="X">` subtree become `X::action`.
/// Empty actions and navigation are left untouched.
fn namespace_action(action: String, owner: Option<&str>) -> String {
    match owner {
        Some(name) if !action.is_empty() => format!("{}::{}", name, action),
        _ => action,
    }
}

/// Core of [`evaluate_node`]. `owner` is the name of the nearest enclosing
/// `<Component>`/`<Include>` reference, used to namespace its actions. `scope`
/// is the component whose `<link>`-scoped stylesheets are currently in effect
/// (it follows the same component boundaries as `owner`).
fn eval_owned(
    node: &UiNode,
    context: &HashMap<String, String>,
    templates: &HashMap<String, UiNode>,
    styles: &StyleContext,
    scope: Option<&str>,
    owner: Option<&str>,
) -> Result<UiNode, String> {
    // A component reference — either the legacy `<Include src="..." />` or a tag
    // named after a registered component (e.g. `<PerfilCard ... />`) — is replaced
    // with the evaluated template root, with its attributes passed in as props.
    let reference: Option<(&String, &HashMap<String, String>)> = match &node.kind {
        NodeType::Include { src, props } => Some((src, props)),
        NodeType::Component { name, props } => Some((name, props)),
        _ => None,
    };
    if let Some((name, props)) = reference {
        let template_ast = templates.get(name)
            .ok_or_else(|| format!("Component '{}' not registered", name))?;

        // Create a local context by copying the parent context and merging evaluated properties
        let mut local_context = context.clone();
        for (key, val_template) in props {
            let evaluated_val = process_template(val_template, context);
            local_context.insert(key.clone(), evaluated_val);
        }

        // The referenced subtree's actions and scoped styles belong to `name`
        // (innermost wins).
        return eval_owned(template_ast, &local_context, templates, styles, Some(name), Some(name));
    }

    // Resolve `class="..."` into a merged style rule that sits *underneath* the
    // node's inline attributes (inline wins, per CSS precedence). Global sheets
    // apply first, then the current component's scoped sheets.
    let style: StyleRule = match &node.class {
        Some(class) => {
            let active = styles.active(scope);
            resolve_classes(&process_template(class, context), &active)
        }
        None => StyleRule::default(),
    };

    // Evaluate current node attributes
    let kind_eval = match &node.kind {
        NodeType::Container => NodeType::Container,
        NodeType::Column => NodeType::Column,
        NodeType::Row => NodeType::Row,
        NodeType::Text { content, size, bold, color } => {
            NodeType::Text {
                content: process_template(content, context),
                size: size.or(style.size),
                bold: *bold || style.bold.unwrap_or(false),
                color: color.as_ref()
                    .map(|c| process_template(c, context))
                    .or_else(|| style.color.clone()),
            }
        }
        NodeType::Button { text, on_click, navigate_to, navigate_back, color } => {
            NodeType::Button {
                text: process_template(text, context),
                on_click: on_click.as_ref()
                    .map(|o| namespace_action(process_template(o, context), owner)),
                navigate_to: navigate_to.as_ref().map(|n| process_template(n, context)),
                navigate_back: *navigate_back,
                color: color.as_ref()
                    .map(|c| process_template(c, context))
                    .or_else(|| style.color.clone()),
            }
        }
        NodeType::TextInput { placeholder, value_var, on_change, secure } => {
            NodeType::TextInput {
                placeholder: process_template(placeholder, context),
                value_var: process_template(value_var, context),
                on_change: namespace_action(process_template(on_change, context), owner),
                secure: *secure,
            }
        }
        NodeType::TextArea { placeholder, value_var, on_change } => {
            NodeType::TextArea {
                placeholder: process_template(placeholder, context),
                value_var: process_template(value_var, context),
                on_change: namespace_action(process_template(on_change, context), owner),
            }
        }
        NodeType::Image { source, clip_circle } => {
            NodeType::Image {
                source: process_template(source, context),
                clip_circle: *clip_circle,
            }
        }
        NodeType::Svg { source, color } => {
            NodeType::Svg {
                source: process_template(source, context),
                color: color.as_ref()
                    .map(|c| process_template(c, context))
                    .or_else(|| style.color.clone()),
            }
        }
        NodeType::Scrollable { direction } => NodeType::Scrollable { direction: direction.clone() },
        NodeType::Checkbox { label, checked_var, on_toggle } => {
            NodeType::Checkbox {
                label: process_template(label, context),
                checked_var: process_template(checked_var, context),
                on_toggle: namespace_action(process_template(on_toggle, context), owner),
            }
        }
        NodeType::Toggle { label, checked_var, on_toggle } => {
            NodeType::Toggle {
                label: process_template(label, context),
                checked_var: process_template(checked_var, context),
                on_toggle: namespace_action(process_template(on_toggle, context), owner),
            }
        }
        NodeType::Rule { horizontal } => NodeType::Rule { horizontal: *horizontal },
        NodeType::Select { options, value_var, on_change, placeholder, label_field, value_field, color } => {
            NodeType::Select {
                options: process_template(options, context),
                value_var: process_template(value_var, context),
                on_change: namespace_action(process_template(on_change, context), owner),
                placeholder: process_template(placeholder, context),
                label_field: label_field.clone(),
                value_field: value_field.clone(),
                color: color.as_ref()
                    .map(|c| process_template(c, context))
                    .or_else(|| style.color.clone()),
            }
        }
        NodeType::Include { .. } | NodeType::Component { .. } | NodeType::Import { .. }
        | NodeType::ForEach { .. } | NodeType::If { .. } | NodeType::Else
        | NodeType::Link { .. } => {
            NodeType::Container
        }
    };

    // For each style field, the node's inline attribute wins; a `class` value
    // (if any) fills in only where the inline attribute is absent.
    let resolve = |inline: &Option<String>, class: &Option<String>| -> Option<String> {
        inline
            .as_ref()
            .map(|s| process_template(s, context))
            .or_else(|| class.clone())
    };

    let width_eval = resolve(&node.width, &style.width);
    let height_eval = resolve(&node.height, &style.height);
    let padding_eval = resolve(&node.padding, &style.padding);
    let align_x_eval = resolve(&node.align_x, &style.align_x);
    let align_y_eval = resolve(&node.align_y, &style.align_y);
    let background_eval = resolve(&node.background, &style.background);
    let border_color_eval = resolve(&node.border_color, &style.border_color);
    let spacing_eval = node.spacing.or(style.spacing);
    let border_radius_eval = node.border_radius.or(style.border_radius);
    let border_width_eval = node.border_width.or(style.border_width);
    let font_eval = resolve(&node.font, &style.font);
    let gradient_eval = resolve(&node.gradient, &style.gradient);
    let text_align_eval = resolve(&node.text_align, &style.text_align);
    // `on_press` is behavior, not a style field; interpolate it directly so
    // actions like `onPress="window:{cmd}"` can bind context values.
    let on_press_eval = node.on_press.as_ref().map(|s| process_template(s, context));

    // Evaluate children recursively. ForEach/if/else/Import are structural:
    // they are expanded or dropped rather than rendered directly.
    let mut children_eval = Vec::new();
    expand_children(&node.children, context, templates, styles, scope, owner, &mut children_eval)?;

    Ok(UiNode {
        kind: kind_eval,
        children: children_eval,
        width: width_eval,
        height: height_eval,
        padding: padding_eval,
        align_x: align_x_eval,
        align_y: align_y_eval,
        spacing: spacing_eval,
        background: background_eval,
        border_radius: border_radius_eval,
        border_width: border_width_eval,
        border_color: border_color_eval,
        // Classes are fully resolved into the fields above; nothing to carry on.
        class: None,
        font: font_eval,
        gradient: gradient_eval,
        text_align: text_align_eval,
        on_press: on_press_eval,
        if_cond: None,
        if_equals: None,
        if_not_equals: None,
        is_else: false,
        for_each: None,
        for_each_var: None,
    })
}
