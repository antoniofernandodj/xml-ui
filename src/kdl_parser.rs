//! KDL template parser for glacier-ui.
//!
//! A leaner, less verbose alternative to the XML templates. The output is the
//! exact same [`UiNode`]/[`NodeType`] tree the XML parser produces, so it feeds
//! the same evaluation/rendering pipeline — the engine only picks this parser
//! based on the `.kdl` file extension.
//!
//! ```kdl
//! theme "styles/theme.json"
//! style "styles/estilos.gss"
//!
//! Container class="card" {
//!     Column class="stack" {
//!         Text "Contador: {valor}" class="subtitle"
//!         Row class="actions" {
//!             Button "-" onClick="decrementar" class="btn btn-danger"
//!             Button "+" onClick="incrementar" class="btn btn-success"
//!         }
//!     }
//! }
//! ```
//!
//! See `PLANO_KDL.md` for the full syntax and the XML equivalences.

use std::collections::HashMap;
use std::sync::RwLock;
use kdl::{KdlDocument, KdlNode, KdlValue};
use crate::parser::{UiNode, NodeType};

/// Parses a full KDL template string into a [`UiNode`].
///
/// Like [`UiNode::parse_xml`], top-level declaration nodes (`theme`, `style`,
/// `import`, `data`) may sit as siblings of the root layout node; they are
/// collected and re-attached as children of the root so the engine's
/// `<link>`/`<import>` processing finds them (they have no visual effect).
///
/// A `script { ... }` block — whose body is Rust, not valid KDL — is stripped
/// textually before parsing, mirroring `strip_script` for XML.
pub fn parse_kdl(input: &str) -> Result<UiNode, String> {
    let stripped = strip_kdl_script(input);
    let split = split_after_close_braces(&stripped);
    let joined = join_kdl_continuations(&split);
    let doc = KdlDocument::parse(&joined).map_err(|e| e.to_string())?;

    let mut decls = Vec::new();
    let mut root: Option<UiNode> = None;
    for node in doc.nodes() {
        // The `script` node is consumed at compile time by the macro; ignore it
        // at runtime (its body was already stripped above, but a bodyless
        // `script` node could still appear).
        if node.name().value().eq_ignore_ascii_case("script") {
            continue;
        }
        if let Some(ui) = node_from_kdl(node) {
            if matches!(ui.kind, NodeType::Import { .. } | NodeType::Link { .. } | NodeType::Style { .. }) {
                decls.push(ui);
            } else if root.is_none() {
                root = Some(ui);
            }
        }
    }

    let mut root = root.ok_or_else(|| "No root element found".to_string())?;
    root.children.extend(decls);
    Ok(root)
}

/// Removes a top-level `script { ... }` block from the source, returning the
/// remaining markup. The body holds Rust code (consumed by `#[component]`), so
/// it must be stripped before KDL parsing or the document would not parse.
fn strip_kdl_script(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    // Find a `script` token followed (after optional whitespace) by `{`.
    let mut search = 0;
    while let Some(rel) = lower[search..].find("script") {
        let start = search + rel;
        // Must be a standalone node name: preceded by start-of-line/whitespace.
        let preceded_ok = start == 0
            || input[..start].chars().next_back().map_or(true, |c| c.is_whitespace());
        let after = start + "script".len();
        let brace_rel = input[after..].find(|c: char| !c.is_whitespace());
        if preceded_ok {
            if let Some(brel) = brace_rel {
                let brace_idx = after + brel;
                if input.as_bytes()[brace_idx] == b'{' {
                    // Walk to the matching closing brace.
                    if let Some(end) = matching_brace(input, brace_idx) {
                        let mut out = String::with_capacity(input.len());
                        out.push_str(&input[..start]);
                        out.push_str(&input[end + 1..]);
                        return out;
                    }
                }
            }
        }
        search = after;
    }
    input.to_string()
}

/// Given the index of an opening `{`, returns the index of its matching `}`,
/// accounting for nested braces. Naive (does not skip braces inside strings),
/// which is adequate for `script` bodies of Rust code.
fn matching_brace(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Inserts a newline after a children-block `}` that is followed by more node
/// content on the same line, so siblings can share a line with the closing
/// brace:
///
/// ```kdl
/// node1 {
///     Text "a"
/// } node2 {
///     Text "b"
/// }
/// ```
///
/// KDL requires a node-terminator (newline, `;`, comment or EOF) after a node's
/// children block, so `} node2 {` is otherwise a parse error. A `}` is left
/// untouched when followed only by whitespace, a `;`, or a comment — those are
/// already valid terminators. Braces inside `"…"` or `"""…"""` strings (e.g. the
/// `.gss` body of an inline `style`) and inside `//` / `/* … */` comments are
/// ignored.
fn split_after_close_braces(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len() + 16);
    let mut i = 0;
    let mut in_str = false; // inside a "…" single-line string
    let mut in_ml = false; // inside a """…""" multi-line string
    let mut in_line_comment = false; // inside a `//` comment, until newline
    let mut in_block_comment = false; // inside a `/* … */` comment

    while i < chars.len() {
        let c = chars[i];

        // Comments: pass through; their braces must never trigger a split.
        if in_line_comment {
            out.push(c);
            i += 1;
            if c == '\n' {
                in_line_comment = false;
            }
            continue;
        }
        if in_block_comment {
            out.push(c);
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                out.push('/');
                i += 2;
                in_block_comment = false;
            } else {
                i += 1;
            }
            continue;
        }

        // A `"""` toggles the multi-line string state (never while in a `"…"`).
        if c == '"' && !in_str && i + 2 < chars.len() && chars[i + 1] == '"' && chars[i + 2] == '"' {
            in_ml = !in_ml;
            out.push_str("\"\"\"");
            i += 3;
            continue;
        }
        if in_ml {
            out.push(c);
            i += 1;
            continue;
        }
        if in_str {
            // Keep escape sequences intact so `\"` doesn't close the string.
            if c == '\\' && i + 1 < chars.len() {
                out.push(c);
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_str = false;
            }
            out.push(c);
            i += 1;
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
            i += 1;
            continue;
        }
        // Comment openers (outside any string).
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            in_line_comment = true;
            out.push_str("//");
            i += 2;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            in_block_comment = true;
            out.push_str("/*");
            i += 2;
            continue;
        }

        out.push(c);
        i += 1;

        if c == '}' {
            // Peek past spaces/tabs on the same line.
            let mut j = i;
            while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
                j += 1;
            }
            // Break only before more node content — not before a `;`, a newline,
            // a comment, or end of input (all already valid terminators).
            let breaks = match chars.get(j) {
                None | Some('\n') | Some('\r') | Some(';') => false,
                Some('/') if matches!(chars.get(j + 1), Some('/') | Some('*')) => false,
                Some(_) => true,
            };
            if breaks {
                out.push('\n');
            }
        }
    }

    out
}

/// Folds a node's entries that were written across several lines back onto the
/// node's first line, so KDL templates can break a long node over multiple lines
/// **without** trailing `\` continuations:
///
/// ```kdl
/// CartaoKdl
///     nome="Mateus Rocha"
///     cargo="Gerente de Produto"
///     cor="#A6E3A1"            // closes at the dedent / next sibling
///
/// CartaoKdl
///     nome="Ana"
///     cor="#89B4FA";           // or close explicitly with `;`
///
/// CartaoKdl
///     nome="Léo"
///     cor="#F38BA8" {          // or open a children block
///         Text "extra"
///     }
/// ```
///
/// A line is treated as a **continuation** of the node started above it when its
/// first token is a property (`key=…`) or an opening `{`. The node closes on a
/// `;`, a `{ … }` block, a blank line, or the next line that is itself a new
/// node. The legacy `\` line-continuation is still honoured (the backslash is
/// stripped and the next line is folded unconditionally), so old templates keep
/// working.
///
/// Lines inside a `"""` multi-line string (e.g. an inline `style` block whose
/// body is `.gss`) are passed through untouched.
fn join_kdl_continuations(input: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    // Index in `out` of the node line currently eligible to absorb continuations.
    let mut open: Option<usize> = None;
    // Set by a trailing `\`: the next line folds in regardless of its first token.
    let mut forced = false;
    // Inside a `"""` multi-line string, where nothing is folded.
    let mut in_ml = false;

    for raw in input.lines() {
        let trimmed = raw.trim();
        let toggles_ml = trimmed.matches("\"\"\"").count() % 2 == 1;

        // Pass multi-line string contents (and the delimiters) through verbatim.
        if in_ml {
            out.push(raw.to_string());
            if toggles_ml { in_ml = false; }
            open = None;
            forced = false;
            continue;
        }
        if toggles_ml {
            out.push(raw.to_string());
            in_ml = true;
            open = None;
            forced = false;
            continue;
        }

        // A blank line ends any open node.
        if trimmed.is_empty() {
            out.push(raw.to_string());
            open = None;
            forced = false;
            continue;
        }

        // Comments are emitted as-is and don't disturb the open node.
        if trimmed.starts_with("//") || trimmed.starts_with("/*") {
            out.push(raw.to_string());
            continue;
        }

        let is_cont = open.is_some()
            && (forced
                || starts_with_property(trimmed)
                || starts_with_bare_flag(trimmed)
                || trimmed.starts_with('{'));

        if is_cont {
            let idx = open.unwrap();
            out[idx].push(' ');
            out[idx].push_str(trimmed);
            (open, forced) = classify_node_line(&mut out[idx], idx);
        } else {
            out.push(raw.to_string());
            let last = out.len() - 1;
            (open, forced) = classify_node_line(&mut out[last], last);
        }
    }

    out.join("\n")
}

/// Strips a trailing `\` line-continuation from `line` (an emitted node line) and
/// reports, for the folding pass: whether the node stays open to further
/// continuations (`Some(idx)`), and whether the next line must be folded
/// unconditionally (`true` only right after a `\`). A node that ends in `;`,
/// `{`, or `}` — or that is a lone `}` — is closed.
fn classify_node_line(line: &mut String, idx: usize) -> (Option<usize>, bool) {
    let t = line.trim_end();
    if let Some(without) = t.strip_suffix('\\') {
        *line = without.trim_end().to_string();
        return (Some(idx), true);
    }
    if t.starts_with('}') || t.ends_with('{') || t.ends_with(';') || t.ends_with('}') {
        return (None, false);
    }
    (Some(idx), false)
}

/// Whether a line begins with a KDL **property** (`key=value`) — i.e. its first
/// token is a bare identifier or quoted string immediately followed by `=`. A
/// node name is never followed by `=`, so this distinguishes a continuation
/// (`nome="Ana"`) from the start of a new node (`Text "Ana"`).
fn starts_with_property(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    if bytes.first() == Some(&b'"') {
        // Quoted key: skip to the closing quote.
        i += 1;
        while i < bytes.len() && bytes[i] != b'"' {
            i += 1;
        }
        if i >= bytes.len() {
            return false;
        }
        i += 1; // consume the closing quote
    } else {
        let start = i;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c.is_alphanumeric() || matches!(c, '_' | '-' | '.') {
                i += 1;
            } else {
                break;
            }
        }
        if i == start {
            return false;
        }
    }
    // Optional spaces, then an `=` makes it a property.
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    bytes.get(i) == Some(&b'=')
}

/// The built-in bare boolean-flag keywords that may lead a continuation line —
/// positional arguments rather than `key=value` properties. These are intrinsic
/// **framework** flags (`bold` on a `Text`, `navigateBack` on a `Button`), so
/// they fold correctly with no client setup.
///
/// Application-level flags (e.g. `secure`, `else`) are intentionally *not* here:
/// an app registers the ones its templates place on their own line via
/// [`register_bare_flags`]. Folding only needs to know a flag when it leads a
/// continuation line; a flag written inline (`Text "x" bold`) parses natively
/// regardless, so the built-in set stays minimal.
const BARE_FLAGS: &[&str] = &[
    "bold", "negrito",
    "navigateBack", "navigate_back", "navigate-back", "voltar",
];

/// Extra bare flags registered by client code, on top of [`BARE_FLAGS`].
static EXTRA_BARE_FLAGS: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// Registers additional bare boolean-flag keywords recognised by the KDL
/// continuation folder, on top of the built-in [`BARE_FLAGS`].
///
/// A bare flag is a positional argument written without a value (e.g. `secure`,
/// `else`). When such a flag sits on its own continuation line, the folder needs
/// to know it is a flag — otherwise it is misread as the start of a new sibling
/// node and swallows the node's remaining properties. Built-in widgets are
/// covered out of the box; call this once at startup if your custom components
/// accept their own bare flags. Matching is case-insensitive; empty strings and
/// duplicates (including built-ins) are ignored, so it is safe to re-register.
///
/// ```
/// glacier_ui::register_bare_flags(["readonly", "required"]);
/// ```
pub fn register_bare_flags<I, S>(flags: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut extra = EXTRA_BARE_FLAGS.write().unwrap();
    for f in flags {
        let f = f.as_ref().trim();
        if f.is_empty()
            || BARE_FLAGS.iter().any(|b| b.eq_ignore_ascii_case(f))
            || extra.iter().any(|e| e.eq_ignore_ascii_case(f))
        {
            continue;
        }
        extra.push(f.to_string());
    }
}

/// Whether `s` begins with a known bare flag (built-in [`BARE_FLAGS`] or one
/// registered via [`register_bare_flags`]) as its first token (delimited by
/// whitespace or end-of-line). This lets a continuation line such as `secure` or
/// `else class="tab_on"` fold onto the node above it instead of being misread as
/// the start of a new sibling node (which would swallow the node's remaining
/// properties). A line that opens a children block (`else {`) is excluded — that
/// is a real block node, not a flag continuation.
fn starts_with_bare_flag(s: &str) -> bool {
    if s.trim_end().ends_with('{') {
        return false;
    }
    let first = s.split(|c: char| c.is_whitespace()).next().unwrap_or("");
    if first.is_empty() {
        return false;
    }
    if BARE_FLAGS.iter().any(|f| first.eq_ignore_ascii_case(f)) {
        return true;
    }
    EXTRA_BARE_FLAGS
        .read()
        .unwrap()
        .iter()
        .any(|f| f.eq_ignore_ascii_case(first))
}

/// The entries of a KDL node split into positional arguments and named
/// properties, with the same case-insensitive multi-key lookup the XML parser
/// uses for attributes.
struct Attrs {
    /// Positional arguments, in order (e.g. the `"Olá"` in `Text "Olá"`).
    args: Vec<String>,
    /// Named properties (e.g. `size=28` -> `"size" => "28"`).
    props: HashMap<String, String>,
}

impl Attrs {
    fn from(node: &KdlNode) -> Self {
        let mut args = Vec::new();
        let mut props = HashMap::new();
        for entry in node.entries() {
            match entry.name() {
                Some(name) => {
                    props.insert(name.value().to_string(), value_to_string(entry.value()));
                }
                None => args.push(value_to_string(entry.value())),
            }
        }
        Self { args, props }
    }

    /// The first named property matching any of `keys`.
    fn get(&self, keys: &[&str]) -> Option<String> {
        keys.iter().find_map(|k| self.props.get(*k).cloned())
    }

    fn get_f32(&self, keys: &[&str]) -> Option<f32> {
        self.get(keys).and_then(|s| s.parse::<f32>().ok())
    }

    /// A boolean attribute: `bold=true` (property) or the shorthand bare flag
    /// `bold` (a positional argument equal to the key name).
    fn get_bool(&self, keys: &[&str]) -> bool {
        for k in keys {
            if let Some(v) = self.props.get(*k) {
                return v.eq_ignore_ascii_case("true") || v == "1";
            }
        }
        keys.iter().any(|k| self.args.iter().any(|a| a.eq_ignore_ascii_case(k)))
    }

    /// The first positional argument (the content of `Text`, `Button`, ...),
    /// skipping any bare boolean flags so `Text "Olá" bold` still yields `"Olá"`.
    fn content_arg(&self, flags: &[&str]) -> Option<String> {
        self.args
            .iter()
            .find(|a| !flags.iter().any(|f| a.eq_ignore_ascii_case(f)))
            .cloned()
    }
}

/// Renders a `KdlValue` as a plain string for use as an attribute/content value.
/// Quoted KDL strings and bare identifiers (`align=Center`) both arrive as
/// `String`; numbers and booleans are stringified.
fn value_to_string(v: &KdlValue) -> String {
    match v {
        KdlValue::String(s) => s.clone(),
        KdlValue::Integer(i) => i.to_string(),
        KdlValue::Float(f) => f.to_string(),
        KdlValue::Bool(b) => b.to_string(),
        KdlValue::Null => String::new(),
    }
}

/// Builds a leaf [`UiNode`] of the given kind with no children or layout attrs.
fn blank(kind: NodeType) -> UiNode {
    UiNode {
        kind,
        children: Vec::new(),
        width: None,
        height: None,
        padding: None,
        align_x: None,
        align_y: None,
        spacing: None,
        background: None,
        border_radius: None,
        border_width: None,
        border_color: None,
        class: None,
        font: None,
        gradient: None,
        text_align: None,
        on_press: None,
        on_double_click: None,
        cursor: None,
        if_cond: None,
        if_equals: None,
        if_not_equals: None,
        is_else: false,
        for_each: None,
        for_each_var: None,
    }
}

/// Converts a single KDL node into a [`UiNode`], recursing into its children.
/// Mirrors `UiNode::from_node` (the XML path), tag-for-tag.
fn node_from_kdl(node: &KdlNode) -> Option<UiNode> {
    let tag = node.name().value();
    let attrs = Attrs::from(node);

    // --- Declaration nodes (top-of-file), mapped onto Link/Import. ---
    match tag {
        "theme" | "Theme" => {
            let href = decl_href(&attrs);
            return Some(blank(NodeType::Link { rel: "theme".to_string(), href, name: None }));
        }
        "style" | "Style" | "stylesheet" | "Stylesheet" => {
            // `style "styles/app.gss"` links an external sheet; a multi-line
            // argument carrying actual `.gss` source (recognised by a `{` or a
            // newline, neither of which appears in a path) is an inline,
            // component-scoped sheet:
            //
            //     style """
            //     .card { padding: 16; }
            //     """
            let arg = decl_href(&attrs);
            if arg.contains('{') || arg.contains('\n') {
                return Some(blank(NodeType::Style { css: arg }));
            }
            return Some(blank(NodeType::Link { rel: "stylesheet".to_string(), href: arg, name: None }));
        }
        "data" | "Data" => {
            let href = decl_href(&attrs);
            let name = attrs.get(&["as", "name", "nome"]);
            return Some(blank(NodeType::Link { rel: "data".to_string(), href, name }));
        }
        "import" | "Import" | "importar" | "Importar" => {
            let name = attrs
                .content_arg(&[])
                .or_else(|| attrs.get(&["name", "nome", "as"]))
                .unwrap_or_default();
            let from = attrs.get(&["from", "de", "src", "path", "caminho"]).unwrap_or_default();
            return Some(blank(NodeType::Import { name, from }));
        }
        "link" | "Link" => {
            // Explicit `link` node, for parity with XML's `<link rel=...>`.
            let rel = attrs.get(&["rel", "tipo"]).unwrap_or_else(|| "stylesheet".to_string());
            let href = decl_href(&attrs);
            let name = attrs.get(&["as", "name", "nome"]);
            return Some(blank(NodeType::Link { rel, href, name }));
        }
        _ => {}
    }

    // --- Layout/style attributes shared by every node. ---
    let width = attrs.get(&["width", "largura", "w"]);
    let height = attrs.get(&["height", "altura", "h"]);
    let padding = attrs.get(&["padding", "espacamento_interno"]);
    let align_x = attrs.get(&["alignX", "align_x", "align-x", "alinhamento_x"]);
    let align_y = attrs.get(&["alignY", "align_y", "align-y", "alinhamento_y"]);
    let spacing = attrs.get_f32(&["spacing", "espacamento"]);
    let background = attrs.get(&["background", "bg", "fundo"]);
    let border_radius = attrs.get_f32(&["borderRadius", "border_radius", "border-radius", "raio_borda"]);
    let border_width = attrs.get_f32(&["borderWidth", "border_width", "border-width", "largura_borda"]);
    let border_color = attrs.get(&["borderColor", "border_color", "border-color", "cor_borda"]);
    let class = attrs.get(&["class", "classe"]);
    let font = attrs.get(&["font", "fonte", "fontFamily", "font-family"]);
    let gradient = attrs.get(&["gradient", "gradiente"]);
    let text_align = attrs.get(&["textAlign", "text_align", "text-align", "alinhamento_texto"]);
    let on_press = attrs.get(&["onPress", "on_press", "on-press", "aoPressionar", "ao_pressionar"]);
    let on_double_click = attrs.get(&["onDoubleClick", "on_double_click", "on-double-click", "aoClicarDuplo"]);
    let cursor = attrs.get(&["cursor", "cursorIcon", "cursor-icon"]);

    // Structural directives expressed as attributes (Vue/Angular style).
    let if_cond = attrs.get(&["if", "se"]);
    let if_equals = attrs.get(&["equals", "eq", "igual_a"]);
    let if_not_equals = attrs.get(&["notEquals", "not_equals", "ne", "diferente_de"]);
    let is_else = attrs.get_bool(&["else", "senao"]);
    let for_each = attrs.get(&["for-each", "forEach", "foreach", "each", "repeat"]);
    let for_each_var = attrs.get(&["var", "variavel"]);

    let kind = match tag {
        "Container" | "container" => NodeType::Container,
        "Column" | "column" => NodeType::Column,
        "Row" | "row" => NodeType::Row,
        "Text" | "text" => {
            let content = attrs
                .content_arg(&["bold", "negrito"])
                .or_else(|| attrs.get(&["content", "conteudo", "text", "texto"]))
                .unwrap_or_default();
            let size = attrs.get_f32(&["size", "tamanho"]);
            let bold = attrs.get_bool(&["bold", "negrito"]);
            let color = attrs.get(&["color", "cor"]);
            NodeType::Text { content, size, bold, color }
        }
        "Button" | "button" | "Botao" | "botao" => {
            let text = attrs
                .content_arg(&[])
                .or_else(|| attrs.get(&["text", "texto", "content", "conteudo"]))
                .unwrap_or_default();
            let on_click = attrs.get(&["onClick", "on_click", "on-click", "aoClicar", "ao_clicar"]);
            let navigate_to = attrs.get(&["navigateTo", "navigate_to", "navigate-to", "irPara", "ir_para"]);
            let navigate_back = attrs.get_bool(&["navigateBack", "navigate_back", "navigate-back", "voltar"]);
            let color = attrs.get(&["color", "cor"]);
            NodeType::Button { text, on_click, navigate_to, navigate_back, color }
        }
        "TextInput" | "textinput" | "Input" | "input" | "EntradaTexto" | "entrada_texto" => {
            let placeholder = attrs
                .content_arg(&["secure", "password", "seguro", "senha"])
                .or_else(|| attrs.get(&["placeholder", "dica"]))
                .unwrap_or_default();
            let value_var = attrs.get(&["value", "valor"]).unwrap_or_default();
            let on_change = attrs.get(&["onChange", "on_change", "on-change", "aoMudar", "ao_mudar"]).unwrap_or_default();
            let secure = attrs.get_bool(&["secure", "password", "seguro", "senha"]);
            NodeType::TextInput { placeholder, value_var, on_change, secure }
        }
        "TextArea" | "textarea" | "TextEditor" | "texteditor" | "Editor" | "editor" | "AreaTexto" | "area_texto" => {
            let placeholder = attrs
                .content_arg(&[])
                .or_else(|| attrs.get(&["placeholder", "dica"]))
                .unwrap_or_default();
            let value_var = attrs.get(&["value", "valor"]).unwrap_or_default();
            let on_change = attrs.get(&["onChange", "on_change", "on-change", "aoMudar", "ao_mudar"]).unwrap_or_default();
            NodeType::TextArea { placeholder, value_var, on_change }
        }
        "Image" | "image" | "Imagem" | "imagem" => {
            let source = attrs
                .content_arg(&[])
                .or_else(|| attrs.get(&["source", "src", "origem", "caminho"]))
                .unwrap_or_default();
            let clip = attrs.get(&["clip", "corte"]);
            let clip_circle = clip.map(|s| s.eq_ignore_ascii_case("circle")).unwrap_or(false);
            NodeType::Image { source, clip_circle }
        }
        "Svg" | "svg" | "Icon" | "icon" | "Icone" | "icone" => {
            let source = attrs
                .content_arg(&[])
                .or_else(|| attrs.get(&["source", "src", "origem", "caminho"]))
                .unwrap_or_default();
            let color = attrs.get(&["color", "cor"]);
            NodeType::Svg { source, color }
        }
        "Scrollable" | "scrollable" | "Scroll" | "scroll" | "Rolagem" | "rolagem" => {
            let direction = attrs.get(&["direction", "direcao", "axis", "eixo"])
                .unwrap_or_else(|| "vertical".to_string());
            NodeType::Scrollable { direction }
        }
        "Checkbox" | "checkbox" | "Check" | "check" => {
            let label = attrs
                .content_arg(&[])
                .or_else(|| attrs.get(&["label", "text", "texto", "rotulo"]))
                .unwrap_or_default();
            let checked_var = attrs.get(&["checked", "value", "valor", "marcado"]).unwrap_or_default();
            let on_toggle = attrs.get(&["onToggle", "on_toggle", "on-toggle", "onChange", "aoMudar"]).unwrap_or_default();
            NodeType::Checkbox { label, checked_var, on_toggle }
        }
        "Toggle" | "toggle" | "Toggler" | "toggler" | "Switch" | "switch" => {
            let label = attrs
                .content_arg(&[])
                .or_else(|| attrs.get(&["label", "text", "texto", "rotulo"]))
                .unwrap_or_default();
            let checked_var = attrs.get(&["checked", "value", "valor", "marcado"]).unwrap_or_default();
            let on_toggle = attrs.get(&["onToggle", "on_toggle", "on-toggle", "onChange", "aoMudar"]).unwrap_or_default();
            NodeType::Toggle { label, checked_var, on_toggle }
        }
        "Rule" | "rule" | "Divider" | "divider" | "Divisoria" | "divisoria" => {
            let horizontal = attrs.get(&["direction", "direcao", "axis", "eixo"])
                .map(|s| !s.eq_ignore_ascii_case("vertical") && !s.eq_ignore_ascii_case("v"))
                .unwrap_or(true);
            NodeType::Rule { horizontal }
        }
        "Select" | "select" | "Dropdown" | "dropdown" | "PickList" | "picklist"
        | "ComboBox" | "combobox" | "Combo" | "combo" | "Seletor" | "seletor" => {
            let options = attrs.get(&["options", "items", "itens", "source", "origem", "opcoes"]).unwrap_or_default();
            let value_var = attrs.get(&["value", "valor", "selected", "selecionado"]).unwrap_or_default();
            let on_change = attrs.get(&["onChange", "on_change", "on-change", "onSelect", "aoMudar", "ao_mudar"]).unwrap_or_default();
            let placeholder = attrs.get(&["placeholder", "dica"]).unwrap_or_default();
            let label_field = attrs.get(&["labelField", "label_field", "label-field", "labelKey", "campo_rotulo"]).unwrap_or_else(|| "label".to_string());
            let value_field = attrs.get(&["valueField", "value_field", "value-field", "valueKey", "campo_valor"]).unwrap_or_else(|| "value".to_string());
            let color = attrs.get(&["color", "cor"]);
            NodeType::Select { options, value_var, on_change, placeholder, label_field, value_field, color }
        }
        "Include" | "include" | "Incluir" | "incluir" => {
            let src = attrs
                .content_arg(&[])
                .or_else(|| attrs.get(&["src", "fonte"]))
                .unwrap_or_default();
            let mut props = attrs.props.clone();
            props.remove("src");
            props.remove("fonte");
            NodeType::Include { src, props }
        }
        "ForEach" | "foreach" | "For" | "for" => {
            let items = attrs.get(&["items", "itens", "source", "origem"]).unwrap_or_default();
            let var = attrs.get(&["var", "variavel"]).unwrap_or_default();
            NodeType::ForEach { items, var }
        }
        "If" | "if" | "Se" | "se" => {
            let cond = attrs
                .content_arg(&[])
                .or_else(|| attrs.get(&["cond", "condition", "when", "quando", "condicao"]))
                .unwrap_or_default();
            let equals = attrs.get(&["equals", "eq", "igual_a"]);
            let not_equals = attrs.get(&["notEquals", "not_equals", "ne", "diferente_de"]);
            NodeType::If { cond, equals, not_equals }
        }
        "Else" | "else" | "Senao" | "senao" => NodeType::Else,
        _ => {
            // Any unknown tag is a reference to another component by its own
            // name (e.g. `PerfilCard nome="..."`). All props become forwarded.
            NodeType::Component {
                name: tag.to_string(),
                props: attrs.props.clone(),
            }
        }
    };

    // Recurse into the child block, if any.
    let mut children = Vec::new();
    if let Some(doc) = node.children() {
        for child in doc.nodes() {
            if child.name().value().eq_ignore_ascii_case("script") {
                continue;
            }
            if let Some(child_node) = node_from_kdl(child) {
                children.push(child_node);
            }
        }
    }

    Some(UiNode {
        kind,
        children,
        width,
        height,
        padding,
        align_x,
        align_y,
        spacing,
        background,
        border_radius,
        border_width,
        border_color,
        class,
        font,
        gradient,
        text_align,
        on_press,
        on_double_click,
        cursor,
        if_cond,
        if_equals,
        if_not_equals,
        is_else,
        for_each,
        for_each_var,
    })
}

/// The resource path of a declaration node: the first positional argument
/// (`theme "styles/theme.json"`) or, failing that, an explicit `href`/`src`.
fn decl_href(attrs: &Attrs) -> String {
    attrs
        .content_arg(&[])
        .or_else(|| attrs.get(&["href", "src", "from", "caminho"]))
        .unwrap_or_default()
}
