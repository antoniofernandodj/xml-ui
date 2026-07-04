//! Glacier Stylesheet (`.gss`): a small, CSS-like format that lifts repeated
//! style attributes out of the XML markup and into reusable classes.
//!
//! ```gss
//! // styles/app.gss
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
    pub font: Option<String>,
    pub gradient: Option<String>,
    pub text_align: Option<String>,
    pub cursor: Option<String>,
    /// Cor do rótulo de um `Button` (o `color` do botão pinta o *fundo*).
    pub text_color: Option<String>,
    /// Teto de largura/altura — o elemento é envolto num `container` que limita
    /// (Row/Column do iced não capam o próprio tamanho).
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
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
        if other.font.is_some() { self.font = other.font.clone(); }
        if other.gradient.is_some() { self.gradient = other.gradient.clone(); }
        if other.text_align.is_some() { self.text_align = other.text_align.clone(); }
        if other.cursor.is_some() { self.cursor = other.cursor.clone(); }
        if other.text_color.is_some() { self.text_color = other.text_color.clone(); }
        if other.max_width.is_some() { self.max_width = other.max_width; }
        if other.max_height.is_some() { self.max_height = other.max_height; }
    }

    /// Resolve `var(--x)` em todos os campos String da regra contra `vars`
    /// (os campos numéricos não suportam `var()`).
    fn resolve_var_refs(&mut self, vars: &HashMap<String, String>) {
        let sub = |o: &mut Option<String>| {
            if let Some(v) = o {
                if v.contains("var(") {
                    *v = substitute_vars(v, vars);
                }
            }
        };
        sub(&mut self.width);
        sub(&mut self.height);
        sub(&mut self.padding);
        sub(&mut self.align_x);
        sub(&mut self.align_y);
        sub(&mut self.background);
        sub(&mut self.border_color);
        sub(&mut self.color);
        sub(&mut self.font);
        sub(&mut self.gradient);
        sub(&mut self.text_align);
        sub(&mut self.cursor);
        sub(&mut self.text_color);
    }
}

/// Substitui `var(--nome)` / `var(--nome, fallback)` num valor pelo valor da
/// variável (ou pelo fallback, ou string vazia se nenhum existir). Uma passada,
/// sem recursão (uma variável não referencia outra). `var(` sem `)` é deixado
/// como está.
fn substitute_vars(value: &str, vars: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("var(") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        let Some(end) = after.find(')') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = after[..end].trim();
        let (name, fallback) = match inner.split_once(',') {
            Some((n, f)) => (n.trim(), Some(f.trim())),
            None => (inner, None),
        };
        let replacement = vars
            .get(name)
            .map(String::as_str)
            .or(fallback)
            .unwrap_or("");
        out.push_str(replacement);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// A parsed `.gss` document: a map from class name (without the leading `.`)
/// to its [`StyleRule`], plus the design tokens declared in a `:root { --x }`
/// block (referenced elsewhere as `var(--x)`).
#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    pub rules: HashMap<String, StyleRule>,
    /// Variáveis/design tokens de `:root { --nome: valor; }`, sem o `--`… não:
    /// a chave guarda o `--nome` completo (como aparece em `var(--nome)`).
    pub variables: HashMap<String, String>,
}

impl StyleSheet {
    /// Parses an `.gss` source string into a [`StyleSheet`].
    pub fn parse(input: &str) -> Result<Self, String> {
        parse_gss(input)
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
    // Design tokens (`:root { --x }`) de TODOS os sheets ativos, later-sheet
    // vence — assim uma paleta declarada uma vez (ex.: no `app.gss` global)
    // resolve `var(--x)` em qualquer regra, inclusive de sheets com escopo.
    // Substituição no fim (já com a regra mesclada), uma única vez.
    let mut vars: HashMap<String, String> = HashMap::new();
    for sheet in sheets {
        for (k, v) in &sheet.variables {
            vars.insert(k.clone(), v.clone());
        }
    }
    // Sempre resolve (o método só toca campos que contêm `var(`): mesmo sem
    // nenhuma variável, um `var(--x, fallback)` deve cair no fallback.
    merged.resolve_var_refs(&vars);
    merged
}

/// Removes `//` line comments and `/* ... */` block comments from an `.gss`
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

/// Parses an `.gss` document.
///
/// Grammar (intentionally tiny):
/// - Comments: `//` to end of line, and `/* ... */` blocks (which may span
///   multiple lines). `#` is never a comment, so `#RRGGBB` color values are
///   kept verbatim.
/// - Rules: `.name { prop: value; prop: value; }`
/// - Properties: `key: value;` where the value may contain spaces (`padding: 8 16`).
pub fn parse_gss(input: &str) -> Result<StyleSheet, String> {
    // Strip comments first; '#' inside a value (hex colors) survives.
    let cleaned = strip_comments(input)?;

    let mut rules: HashMap<String, StyleRule> = HashMap::new();
    let mut variables: HashMap<String, String> = HashMap::new();
    let mut rest = cleaned.as_str();
    while let Some(open) = rest.find('{') {
        let selector = rest[..open].trim();
        let after_open = &rest[open + 1..];
        let close = after_open
            .find('}')
            .ok_or_else(|| format!("Unclosed rule for selector '{}'", selector))?;
        let body = &after_open[..close];
        rest = &after_open[close + 1..];

        // `:root { --nome: valor; }` — bloco especial de design tokens (a única
        // exceção à regra "só seletor de classe"). Referenciados com `var(--nome)`.
        if selector == ":root" {
            parse_root_vars(body, &mut variables)?;
            continue;
        }

        if !selector.starts_with('.') {
            return Err(format!(
                "Selector '{}' must start with '.' (only class selectors and ':root' are supported)",
                selector
            ));
        }
        let name = selector[1..].trim().to_string();
        if name.is_empty() {
            return Err("Empty class selector '.'".to_string());
        }
        let rule = parse_rule_body(body, &name)?;
        // Classe duplicada no mesmo arquivo faz *merge* (não clobber): o CSS
        // aplica ambas as regras de mesmo seletor. Sobrescrever a anterior
        // inteira era um footgun silencioso. Campos `None` do 2º bloco
        // preservam os do 1º; campos `Some` sobrescrevem.
        rules.entry(name).or_default().merge_from(&rule);
    }

    // Anything left after the last rule that isn't blank is a dangling selector.
    if !rest.trim().is_empty() {
        return Err(format!("Expected '{{' after selector '{}'", rest.trim()));
    }

    Ok(StyleSheet { rules, variables })
}

/// Parses the `--nome: valor;` declarations of a `:root { ... }` block into the
/// sheet's design-token map. As chaves guardam o `--nome` completo.
fn parse_root_vars(body: &str, vars: &mut HashMap<String, String>) -> Result<(), String> {
    for decl in body.split(';') {
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }
        let (key, value) = decl
            .split_once(':')
            .ok_or_else(|| format!("Invalid variable declaration '{}' in ':root'", decl))?;
        let key = key.trim();
        let value = value.trim();
        if !key.starts_with("--") {
            return Err(format!(
                "Variable '{}' in ':root' must start with '--'",
                key
            ));
        }
        if value.is_empty() {
            return Err(format!("Empty value for variable '{}' in ':root'", key));
        }
        vars.insert(key.to_string(), value.to_string());
    }
    Ok(())
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
            "font" | "font-family" | "font_family" => rule.font = Some(value),
            "gradient" => rule.gradient = Some(value),
            "text-align" | "text_align" | "textAlign" => rule.text_align = Some(value),
            "cursor" | "cursor-icon" | "cursorIcon" => rule.cursor = Some(value),
            "text-color" | "text_color" | "textColor" => rule.text_color = Some(value),
            "max-width" | "max_width" | "maxWidth" => rule.max_width = Some(parse_f32(&value)?),
            "max-height" | "max_height" | "maxHeight" => rule.max_height = Some(parse_f32(&value)?),
            // Propriedade desconhecida: pular com aviso, sem derrubar o arquivo
            // inteiro. Um typo (`colr:`) não deve apagar todas as regras do
            // sheet — o resto da regra e das outras regras continua válido.
            // Erros *estruturais* (sem `:`, valor vazio, número inválido)
            // seguem sendo erro fatal.
            other => {
                eprintln!(
                    "glacier-ui: propriedade GSS desconhecida '{}' em '.{}' (ignorada)",
                    other, selector
                );
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
        let gss = "
            // a comment
            .card {
                background: #2E3440;
                border-radius: 12;
                padding: 16;
            }
        ";
        let sheet = parse_gss(gss).unwrap();
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
        let sheet = parse_gss(css).unwrap();
        let card = sheet.rules.get("card").unwrap();
        assert_eq!(card.padding.as_deref(), Some("16"));
        assert_eq!(card.color.as_deref(), Some("#2E3440"));
    }

    #[test]
    fn block_comment_does_not_glue_tokens() {
        // The comment between `.a` rules must not merge them into one selector.
        let sheet = parse_gss(".a { padding: 1; }/* x */.b { padding: 2; }").unwrap();
        assert_eq!(sheet.rules["a"].padding.as_deref(), Some("1"));
        assert_eq!(sheet.rules["b"].padding.as_deref(), Some("2"));
    }

    #[test]
    fn unterminated_block_comment_is_an_error() {
        assert!(parse_gss(".a { padding: 1; } /* oops").is_err());
    }

    #[test]
    fn multi_value_padding_is_preserved() {
        let sheet = parse_gss(".btn { padding: 8 16; }").unwrap();
        assert_eq!(sheet.rules["btn"].padding.as_deref(), Some("8 16"));
    }

    #[test]
    fn classes_merge_left_to_right_then_files() {
        let base = parse_gss(".a { padding: 4; color: #111; }").unwrap();
        let over = parse_gss(".b { color: #222; } .a { padding: 8; }").unwrap();
        let merged = resolve_classes("a b", &[&base, &over]);
        // `.a` padding is overridden by the later sheet; `.b` color wins over `.a`.
        assert_eq!(merged.padding.as_deref(), Some("8"));
        assert_eq!(merged.color.as_deref(), Some("#222"));
    }

    #[test]
    fn unknown_property_is_skipped_not_fatal() {
        // Um typo/propriedade desconhecida é ignorado (com aviso no stderr),
        // mas o resto da regra e do arquivo continua válido.
        let sheet = parse_gss(".x { wibble: 1; color: #123; }").unwrap();
        assert_eq!(sheet.rules["x"].color.as_deref(), Some("#123"));
    }

    #[test]
    fn duplicate_class_merges_not_clobbers() {
        // Dois blocos `.card`: o 2º mescla sobre o 1º (não apaga). Campos
        // ausentes no 2º preservam os do 1º; presentes sobrescrevem.
        let sheet = parse_gss(".card { padding: 4; color: #111; } .card { color: #222; }").unwrap();
        let card = &sheet.rules["card"];
        assert_eq!(card.padding.as_deref(), Some("4"));  // preservado do 1º bloco
        assert_eq!(card.color.as_deref(), Some("#222")); // sobrescrito pelo 2º
    }

    #[test]
    fn selector_must_be_a_class() {
        assert!(parse_gss("card { padding: 1; }").is_err());
    }

    #[test]
    fn parses_text_color_and_max_size() {
        let sheet = parse_gss(
            ".btn { text-color: #0D1117; } .panel { max-width: 640; max-height: 480; }",
        )
        .unwrap();
        assert_eq!(sheet.rules["btn"].text_color.as_deref(), Some("#0D1117"));
        assert_eq!(sheet.rules["panel"].max_width, Some(640.0));
        assert_eq!(sheet.rules["panel"].max_height, Some(480.0));
    }

    #[test]
    fn root_vars_resolve_via_var() {
        let sheet = parse_gss(
            ":root { --bg: #0D1117; --accent: #58A6FF; } \
             .card { background: var(--bg); color: var(--accent); }",
        )
        .unwrap();
        assert_eq!(sheet.variables["--bg"].as_str(), "#0D1117");
        // A substituição acontece na resolução (resolve_classes), não no parse.
        let r = resolve_classes("card", &[&sheet]);
        assert_eq!(r.background.as_deref(), Some("#0D1117"));
        assert_eq!(r.color.as_deref(), Some("#58A6FF"));
    }

    #[test]
    fn var_fallback_and_undefined() {
        let sheet = parse_gss(".x { color: var(--missing, #FF0000); background: var(--nope); }").unwrap();
        let r = resolve_classes("x", &[&sheet]);
        assert_eq!(r.color.as_deref(), Some("#FF0000")); // usa o fallback
        assert_eq!(r.background.as_deref(), Some("")); // sem var nem fallback → vazio
    }

    #[test]
    fn vars_are_cross_sheet() {
        // Paleta declarada num sheet (global), usada por regra de outro (escopo).
        let global = parse_gss(":root { --ok: #3FB950; }").unwrap();
        let scoped = parse_gss(".state { color: var(--ok); }").unwrap();
        let r = resolve_classes("state", &[&global, &scoped]);
        assert_eq!(r.color.as_deref(), Some("#3FB950"));
    }

    #[test]
    fn var_embedded_in_gradient() {
        let sheet = parse_gss(
            ":root { --a: #000000; --b: #FFFFFF; } .g { gradient: var(--a) var(--b); }",
        )
        .unwrap();
        let r = resolve_classes("g", &[&sheet]);
        assert_eq!(r.gradient.as_deref(), Some("#000000 #FFFFFF"));
    }
}
