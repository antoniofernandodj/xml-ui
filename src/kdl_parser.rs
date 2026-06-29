//! KDL template parser for glacier-ui.
//!
//! A leaner, less verbose alternative to the XML templates. The output is the
//! exact same [`UiNode`]/[`NodeType`] tree the XML parser produces, so it feeds
//! the same evaluation/rendering pipeline — the engine only picks this parser
//! based on the `.kdl` file extension.
//!
//! ```kdl
//! theme "styles/theme.json"
//! style "styles/estilos.iss"
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
    let doc = KdlDocument::parse(&stripped).map_err(|e| e.to_string())?;

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
            if matches!(ui.kind, NodeType::Import { .. } | NodeType::Link { .. }) {
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
            let href = decl_href(&attrs);
            return Some(blank(NodeType::Link { rel: "stylesheet".to_string(), href, name: None }));
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
