//! Iced Stylesheet (`.iss`): a small, CSS-like format that lifts repeated
//! style attributes out of the XML markup and into reusable classes.
//!
//! ```iss
//! // styles/app.iss
//! /* Comentários de bloco também são suportados,
//!    inclusive em várias linhas. */
//! .card {
//!     background: #2E3440;
//!     border-radius: 12;
//!     padding: 16;
//! }
//! ```
//!
//! Used from the XML via `class="card centered"`. Class fields are applied
//! left-to-right; inline attributes on the node always win (same precedence
//! as CSS). See [`StyleSheet`] and [`resolve_classes`].

use std::collections::HashMap;

/// The set of style fields a single `.class { ... }` rule may carry.
///
/// Mirrors the style-bearing fields of [`crate::parser::UiNode`] (plus the
/// `color`/`size`/`bold` of `Text`/`Button`). Every field is optional: a rule
/// only sets the properties it actually declares, leaving the rest to be filled
/// by other classes or by inline attributes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StyleRule {
    pub width: Option<String>,
    pub height: Option<String>,
    pub padding: Option<String>,
    pub spacing: Option<f32>,
    pub align_x: Option<String>,
    pub align_y: Option<String>,
    pub background: Option<String>,
    pub border_radius: Option<f32>,
    pub border_width: Option<f32>,
    pub border_color: Option<String>,
    pub color: Option<String>,
    pub size: Option<f32>,
    pub bold: Option<bool>,
}

impl StyleRule {
    /// Overlays every `Some` field of `other` onto `self`, leaving `self`'s
    /// fields untouched where `other` is `None`. Used to merge classes in order.
    pub fn merge_from(&mut self, other: &StyleRule) {
        if other.width.is_some() { self.width = other.width.clone(); }
        if other.height.is_some() { self.height = other.height.clone(); }
        if other.padding.is_some() { self.padding = other.padding.clone(); }
        if other.spacing.is_some() { self.spacing = other.spacing; }
        if other.align_x.is_some() { self.align_x = other.align_x.clone(); }
        if other.align_y.is_some() { self.align_y = other.align_y.clone(); }
        if other.background.is_some() { self.background = other.background.clone(); }
        if other.border_radius.is_some() { self.border_radius = other.border_radius; }
        if other.border_width.is_some() { self.border_width = other.border_width; }
        if other.border_color.is_some() { self.border_color = other.border_color.clone(); }
        if other.color.is_some() { self.color = other.color.clone(); }
        if other.size.is_some() { self.size = other.size; }
        if other.bold.is_some() { self.bold = other.bold; }
    }
}

/// A parsed `.iss` document: a map from class name (without the leading `.`)
/// to its [`StyleRule`].
#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    pub rules: HashMap<String, StyleRule>,
}

impl StyleSheet {
    /// Parses an `.iss` source string into a [`StyleSheet`].
    pub fn parse(input: &str) -> Result<Self, String> {
        parse_iss(input)
    }
}

/// Merges the named classes (a whitespace-separated `class="a b c"` string)
/// across the given stylesheets into a single [`StyleRule`].
///
/// Classes are applied left-to-right (later classes override earlier ones).
/// For a given class name, later stylesheets in the slice take priority, so
/// callers can layer files by ascending priority (e.g. global sheets first,
/// then a component's own scoped sheets).
pub fn resolve_classes(classes: &str, sheets: &[&StyleSheet]) -> StyleRule {
    let mut merged = StyleRule::default();
    for name in classes.split_whitespace() {
        for sheet in sheets {
            if let Some(rule) = sheet.rules.get(name) {
                merged.merge_from(rule);
            }
        }
    }
    merged
}

/// Removes `//` line comments and `/* ... */` block comments from an `.iss`
/// source, leaving everything else (including `#RRGGBB` colors and newlines)
/// intact. Each block comment is replaced by a single space so it can't glue
/// adjacent tokens together. Errors on an unterminated block comment.
fn strip_comments(input: &str) -> Result<String, String> {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' {
            match chars.peek() {
                // Line comment: drop everything up to (but not including) the newline.
                Some('/') => {
                    chars.next();
                    while let Some(&nc) = chars.peek() {
                        if nc == '\n' {
                            break;
                        }
                        chars.next();
                    }
                    continue;
                }
                // Block comment: drop everything up to and including `*/`.
                Some('*') => {
                    chars.next();
                    let mut closed = false;
                    while let Some(c2) = chars.next() {
                        if c2 == '*' && chars.peek() == Some(&'/') {
                            chars.next();
                            closed = true;
                            break;
                        }
                    }
                    if !closed {
                        return Err("Unterminated block comment `/* ... */`".to_string());
                    }
                    out.push(' ');
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
    Ok(out)
}

/// Parses an `.iss` document.
///
/// Grammar (intentionally tiny):
/// - Comments: `//` to end of line, and `/* ... */` blocks (which may span
///   multiple lines). `#` is never a comment, so `#RRGGBB` color values are
///   kept verbatim.
/// - Rules: `.name { prop: value; prop: value; }`
/// - Properties: `key: value;` where the value may contain spaces (`padding: 8 16`).
pub fn parse_iss(input: &str) -> Result<StyleSheet, String> {
    // Strip comments first; '#' inside a value (hex colors) survives.
    let cleaned = strip_comments(input)?;

    let mut rules = HashMap::new();
    let mut rest = cleaned.as_str();
    while let Some(open) = rest.find('{') {
        let selector = rest[..open].trim();
        let after_open = &rest[open + 1..];
        let close = after_open
            .find('}')
            .ok_or_else(|| format!("Unclosed rule for selector '{}'", selector))?;
        let body = &after_open[..close];
        rest = &after_open[close + 1..];

        if !selector.starts_with('.') {
            return Err(format!(
                "Selector '{}' must start with '.' (only class selectors are supported)",
                selector
            ));
        }
        let name = selector[1..].trim().to_string();
        if name.is_empty() {
            return Err("Empty class selector '.'".to_string());
        }
        let rule = parse_rule_body(body, &name)?;
        rules.insert(name, rule);
    }

    // Anything left after the last rule that isn't blank is a dangling selector.
    if !rest.trim().is_empty() {
        return Err(format!("Expected '{{' after selector '{}'", rest.trim()));
    }

    Ok(StyleSheet { rules })
}

/// Parses the `key: value;` declarations inside a single rule body.
fn parse_rule_body(body: &str, selector: &str) -> Result<StyleRule, String> {
    let mut rule = StyleRule::default();
    for decl in body.split(';') {
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }
        let (key, value) = decl
            .split_once(':')
            .ok_or_else(|| format!("Invalid declaration '{}' in '.{}'", decl, selector))?;
        let key = key.trim();
        let value = value.trim().to_string();
        if value.is_empty() {
            return Err(format!("Empty value for '{}' in '.{}'", key, selector));
        }

        let parse_f32 = |v: &str| -> Result<f32, String> {
            v.parse::<f32>()
                .map_err(|_| format!("Expected a number for '{}' in '.{}', got '{}'", key, selector, v))
        };

        match key {
            "width" | "w" => rule.width = Some(value),
            "height" | "h" => rule.height = Some(value),
            "padding" => rule.padding = Some(value),
            "spacing" => rule.spacing = Some(parse_f32(&value)?),
            "align-x" | "align_x" | "alignX" => rule.align_x = Some(value),
            "align-y" | "align_y" | "alignY" => rule.align_y = Some(value),
            "background" | "bg" => rule.background = Some(value),
            "border-radius" | "border_radius" => rule.border_radius = Some(parse_f32(&value)?),
            "border-width" | "border_width" => rule.border_width = Some(parse_f32(&value)?),
            "border-color" | "border_color" => rule.border_color = Some(value),
            "color" => rule.color = Some(value),
            "size" => rule.size = Some(parse_f32(&value)?),
            "bold" => rule.bold = Some(value.eq_ignore_ascii_case("true") || value == "1"),
            other => {
                return Err(format!(
                    "Unknown style property '{}' in '.{}'",
                    other, selector
                ))
            }
        }
    }
    Ok(rule)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_rule() {
        let css = "
            // a comment
            .card {
                background: #2E3440;
                border-radius: 12;
                padding: 16;
            }
        ";
        let sheet = parse_iss(css).unwrap();
        let card = sheet.rules.get("card").unwrap();
        assert_eq!(card.background.as_deref(), Some("#2E3440"));
        assert_eq!(card.border_radius, Some(12.0));
        assert_eq!(card.padding.as_deref(), Some("16"));
    }

    #[test]
    fn block_comments_are_stripped() {
        let css = "
            /* multi-line
               block comment */
            .card {
                padding: 16; /* trailing block */
                color: #2E3440; // line comment, not the #color
            }
            /* a /*-looking thing that is just text */
        ";
        let sheet = parse_iss(css).unwrap();
        let card = sheet.rules.get("card").unwrap();
        assert_eq!(card.padding.as_deref(), Some("16"));
        assert_eq!(card.color.as_deref(), Some("#2E3440"));
    }

    #[test]
    fn block_comment_does_not_glue_tokens() {
        // The comment between `.a` rules must not merge them into one selector.
        let sheet = parse_iss(".a { padding: 1; }/* x */.b { padding: 2; }").unwrap();
        assert_eq!(sheet.rules["a"].padding.as_deref(), Some("1"));
        assert_eq!(sheet.rules["b"].padding.as_deref(), Some("2"));
    }

    #[test]
    fn unterminated_block_comment_is_an_error() {
        assert!(parse_iss(".a { padding: 1; } /* oops").is_err());
    }

    #[test]
    fn multi_value_padding_is_preserved() {
        let sheet = parse_iss(".btn { padding: 8 16; }").unwrap();
        assert_eq!(sheet.rules["btn"].padding.as_deref(), Some("8 16"));
    }

    #[test]
    fn classes_merge_left_to_right_then_files() {
        let base = parse_iss(".a { padding: 4; color: #111; }").unwrap();
        let over = parse_iss(".b { color: #222; } .a { padding: 8; }").unwrap();
        let merged = resolve_classes("a b", &[&base, &over]);
        // `.a` padding is overridden by the later sheet; `.b` color wins over `.a`.
        assert_eq!(merged.padding.as_deref(), Some("8"));
        assert_eq!(merged.color.as_deref(), Some("#222"));
    }

    #[test]
    fn unknown_property_is_an_error() {
        assert!(parse_iss(".x { wibble: 1; }").is_err());
    }

    #[test]
    fn selector_must_be_a_class() {
        assert!(parse_iss("card { padding: 1; }").is_err());
    }
}
