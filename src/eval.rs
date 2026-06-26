use std::collections::HashMap;
use crate::parser::{UiNode, NodeType};

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

/// Evaluate an `<If>` condition against the context.
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

/// Recursively evaluate a UiNode tree, resolving templates and placeholders.
pub fn evaluate_node(
    node: &UiNode,
    context: &HashMap<String, String>,
    templates: &HashMap<String, UiNode>,
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

        // Recursively evaluate the referenced template root node
        return evaluate_node(template_ast, &local_context, templates);
    }

    // Evaluate current node attributes
    let kind_eval = match &node.kind {
        NodeType::Container => NodeType::Container,
        NodeType::Column => NodeType::Column,
        NodeType::Row => NodeType::Row,
        NodeType::Text { content, size, bold, color } => {
            NodeType::Text {
                content: process_template(content, context),
                size: *size,
                bold: *bold,
                color: color.as_ref().map(|c| process_template(c, context)),
            }
        }
        NodeType::Button { text, on_click, navigate_to, navigate_back, color } => {
            NodeType::Button {
                text: process_template(text, context),
                on_click: on_click.as_ref().map(|o| process_template(o, context)),
                navigate_to: navigate_to.as_ref().map(|n| process_template(n, context)),
                navigate_back: *navigate_back,
                color: color.as_ref().map(|c| process_template(c, context)),
            }
        }
        NodeType::TextInput { placeholder, value_var, on_change } => {
            NodeType::TextInput {
                placeholder: process_template(placeholder, context),
                value_var: process_template(value_var, context),
                on_change: process_template(on_change, context),
            }
        }
        NodeType::Image { source, clip_circle } => {
            NodeType::Image {
                source: process_template(source, context),
                clip_circle: *clip_circle,
            }
        }
        NodeType::Include { .. } | NodeType::Component { .. } | NodeType::Import { .. }
        | NodeType::ForEach { .. } | NodeType::If { .. } | NodeType::Else => {
            NodeType::Container
        }
    };

    let width_eval = node.width.as_ref().map(|s| process_template(s, context));
    let height_eval = node.height.as_ref().map(|s| process_template(s, context));
    let padding_eval = node.padding.as_ref().map(|s| process_template(s, context));
    let align_x_eval = node.align_x.as_ref().map(|s| process_template(s, context));
    let align_y_eval = node.align_y.as_ref().map(|s| process_template(s, context));
    let background_eval = node.background.as_ref().map(|s| process_template(s, context));
    let border_color_eval = node.border_color.as_ref().map(|s| process_template(s, context));

    // Evaluate children recursively. ForEach/If/Else/Import are structural:
    // they are expanded or dropped rather than rendered directly.
    let mut children_eval = Vec::new();
    // Tracks the result of the immediately preceding `<If>`, so an `<Else>`
    // can bind to it. Reset by any other (non-Else) node.
    let mut last_if: Option<bool> = None;
    for child in &node.children {
        match &child.kind {
            // `<import>` declarations are processed at registration time; drop them here.
            NodeType::Import { .. } => {}
            NodeType::ForEach { items, var } => {
                let items_evaluated = process_template(items, context);
                if let Some(json_str) = context.get(&items_evaluated) {
                    if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(json_str) {
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
                            for sub_child in &child.children {
                                children_eval.push(evaluate_node(sub_child, &local_context, templates)?);
                            }
                        }
                    }
                }
                last_if = None;
            }
            NodeType::If { cond, equals, not_equals } => {
                let truthy = eval_condition(cond, equals, not_equals, context);
                if truthy {
                    for sub_child in &child.children {
                        children_eval.push(evaluate_node(sub_child, context, templates)?);
                    }
                }
                last_if = Some(truthy);
            }
            NodeType::Else => {
                if last_if == Some(false) {
                    for sub_child in &child.children {
                        children_eval.push(evaluate_node(sub_child, context, templates)?);
                    }
                }
                last_if = None;
            }
            _ => {
                children_eval.push(evaluate_node(child, context, templates)?);
                last_if = None;
            }
        }
    }

    Ok(UiNode {
        kind: kind_eval,
        children: children_eval,
        width: width_eval,
        height: height_eval,
        padding: padding_eval,
        align_x: align_x_eval,
        align_y: align_y_eval,
        spacing: node.spacing,
        background: background_eval,
        border_radius: node.border_radius,
        border_width: node.border_width,
        border_color: border_color_eval,
    })
}
