use std::collections::HashMap;
use roxmltree::Node;
use crate::stylesheet::StyleRule;

#[derive(Debug, Clone, PartialEq)]
pub enum NodeType {
    Container,
    Column,
    Row,
    Text {
        content: String,
        size: Option<f32>,
        bold: bool,
        color: Option<String>,
    },
    Button {
        text: String,
        on_click: Option<String>,
        /// Destination screen on click (`navigateTo` attribute).
        navigate_to: Option<String>,
        /// if `true`, goes back to the previous screen (`navigateBack` attribute).
        navigate_back: bool,
        color: Option<String>,
    },
    TextInput {
        placeholder: String,
        value_var: String,
        on_change: String,
        /// Masks the input (passwords/tokens) when true (`secure`/`password`).
        secure: bool,
    },
    /// A multi-line text editor bound to a context key. Unlike [`NodeType::TextInput`]
    /// the engine keeps a stateful `text_editor::Content` for it (keyed by
    /// `value_var`); edits are written back to the context and emit `on_change`.
    TextArea {
        placeholder: String,
        value_var: String,
        on_change: String,
        /// Somente-leitura: o usuário pode selecionar, rolar e mover o cursor,
        /// mas edições (digitar/apagar/colar) são ignoradas. Útil para exibir
        /// texto selecionável/copiável (ex.: logs) sem permitir alteração.
        readonly: bool,
    },
    Image {
        source: String,
        clip_circle: bool,
    },
    /// A vector (SVG) image, e.g. `<Svg source="icons/rocket.svg" />`. Rendered
    /// with `iced`'s svg widget; `color` (inline/class) tints it.
    Svg {
        source: String,
        color: Option<String>,
    },
    /// A scrollable viewport wrapping a single child (like `Container`).
    /// `direction` is `vertical` (default), `horizontal` or `both`.
    Scrollable {
        direction: String,
    },
    /// A checkbox bound to a context key. `checked_var` holds the truthy state;
    /// toggling emits `on_toggle` as an `UiInputChanged` carrying `"true"`/`"false"`.
    Checkbox {
        label: String,
        checked_var: String,
        on_toggle: String,
    },
    /// An on/off toggler. Same binding semantics as [`NodeType::Checkbox`].
    Toggle {
        label: String,
        checked_var: String,
        on_toggle: String,
    },
    /// A separator line. `horizontal` true draws a horizontal rule (default),
    /// false a vertical one. Thickness comes from `width`/`height`.
    Rule {
        horizontal: bool,
    },
    /// A dropdown (`pick_list`) bound to context. `options` is a context key
    /// holding a JSON array (objects with `label_field`/`value_field`, or plain
    /// strings); `value_var` holds the selected value; selecting an option emits
    /// `on_change` as an `UiInputChanged` carrying the chosen option's value.
    Select {
        options: String,
        value_var: String,
        on_change: String,
        placeholder: String,
        label_field: String,
        value_field: String,
        /// Text color (inline `color`/`cor` or resolved from a `.gss` class).
        color: Option<String>,
    },
    /// A form container, e.g. `<Form onSubmit="entrar">...</Form>`. Its
    /// descendant inputs bind to [`crate::forms::Form`]/`FormControl`s by name
    /// via the generic `formControl` attribute (see [`UiNode::form_control`]);
    /// renders like a [`NodeType::Column`]. `name` disambiguates two `<Form>`s
    /// in the same component that happen to share a control name (rare —
    /// optional).
    Form {
        on_submit: Option<String>,
        name: Option<String>,
    },
    Include {
        src: String,
        props: HashMap<String, String>,
    },
    /// A reference to another registered component by its own tag name,
    /// e.g. `<PerfilCard nome="..." />`. Attributes become props.
    Component {
        name: String,
        props: HashMap<String, String>,
    },
    /// Declares that a component named `name` should be loaded from the XML
    /// file at `from`, e.g. `<import name="PerfilCard" from="templates/perfil_card.gv" />`.
    /// Processed at registration time and stripped before rendering.
    Import {
        name: String,
        from: String,
    },
    ForEach {
        items: String,
        var: String,
    },
    /// Conditionally renders its children, e.g.
    /// `<if cond="{logado}">...</if>` (truthy) or
    /// `<if cond="{status}" equals="active">...</if>` (comparison).
    If {
        cond: String,
        equals: Option<String>,
        not_equals: Option<String>,
    },
    /// Renders its children when the immediately preceding `<if>` was false.
    Else,
    /// Declares an external resource to load, e.g.
    /// `<link rel="stylesheet" href="styles/card.gss" />`. `rel` selects the
    /// kind of resource:
    /// - `stylesheet` (default): an `.gss` sheet, applied **globally** (every
    ///   component, regardless of where it's declared) — same as calling
    ///   [`crate::GlacierUI::load_stylesheet`] from Rust. There is no scoped
    ///   form of a linked sheet; use an inline `<style scoped="true">` for that;
    /// - `import`/`component`: another component template (declarative
    ///   equivalent of `<import>`); `name`/`as` names it (defaults to the file
    ///   stem);
    /// - `data`: a JSON file merged into the context under the `name`/`as` key;
    /// - `theme`: a JSON palette applied as the app's `iced::Theme`.
    ///
    /// Processed at registration time and stripped before rendering.
    Link {
        rel: String,
        href: String,
        /// The `name`/`as` attribute: context key for `data`, component name
        /// for `import`/`component`.
        name: Option<String>,
    },
    /// An inline stylesheet, e.g. `<style>.card { padding: 16; }</style>`. The
    /// body is `.gss` source (the same grammar as a linked `.gss` file).
    ///
    /// By default (`scoped: false`) it is promoted to the **global** sheet
    /// set, exactly like `<link rel="stylesheet">` — the only difference is
    /// the source is inline instead of an external file. Marking it
    /// `<style scoped="true">` keeps the old behavior: applied only inside the
    /// declaring component's subtree, layered on top of the global sheets.
    /// Processed at registration time and stripped before rendering.
    Style {
        css: String,
        scoped: bool,
    },
    /// A transparent grouping node: renders its children inline into the parent,
    /// adding no layout box of its own. Produced by [`UiNode::parse_xml`] when a
    /// template has more than one top-level node (so a component template can be
    /// a "fragment" of siblings — e.g. an `if`/`else` pair — without a wrapper),
    /// and also writable explicitly as `Fragment { … }`. During evaluation
    /// (`expand_children`) a `Fragment`'s children are spliced into the
    /// surrounding list, so it normally never reaches rendering; `render_node`
    /// falls back to stacking them in a `Column` if one ever does (e.g. a
    /// multi-root screen root).
    Fragment,
}

impl NodeType {
    /// The canonical (lowercase) tag name a **tag selector** (`Button { }`)
    /// matches this node by — its builtin kind. `None` for reference/structural
    /// nodes (components, imports, `if`/`else`, `Fragment`, …), which either
    /// inline away before styling (components are matched by *name*, as an
    /// underlay, not here) or never render a box. Tag selectors are normalized
    /// to lowercase, so `Button {}` and `button {}` both match a `Button`.
    pub fn tag_name(&self) -> Option<&'static str> {
        Some(match self {
            NodeType::Container => "container",
            NodeType::Column => "column",
            NodeType::Row => "row",
            NodeType::Text { .. } => "text",
            NodeType::Button { .. } => "button",
            NodeType::TextInput { .. } => "textinput",
            NodeType::TextArea { .. } => "textarea",
            NodeType::Image { .. } => "image",
            NodeType::Svg { .. } => "svg",
            NodeType::Scrollable { .. } => "scrollable",
            NodeType::Checkbox { .. } => "checkbox",
            NodeType::Toggle { .. } => "toggle",
            NodeType::Rule { .. } => "rule",
            NodeType::Select { .. } => "select",
            NodeType::Form { .. } => "form",
            NodeType::Include { .. }
            | NodeType::Component { .. }
            | NodeType::Import { .. }
            | NodeType::ForEach { .. }
            | NodeType::If { .. }
            | NodeType::Else
            | NodeType::Link { .. }
            | NodeType::Style { .. }
            | NodeType::Fragment => return None,
        })
    }
}

/// A numeric attribute that normally parses to `f32` at parse time. When its
/// value carries a `{...}` placeholder it can't be parsed until the context is
/// known, so the raw string is stashed in [`UiNode::numeric_templates`] under
/// the matching variant and resolved during evaluation instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumAttr {
    Spacing,
    BorderRadius,
    BorderWidth,
    MaxWidth,
    MaxHeight,
    /// `size` of a `Text` node.
    Size,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiNode {
    pub kind: NodeType,
    pub children: Vec<UiNode>,
    /// Raw, still-templated values of numeric attributes whose XML value held a
    /// `{...}` placeholder (so they couldn't be parsed to `f32` at parse time).
    /// Resolved and parsed during evaluation; see [`NumAttr`]. Empty for the
    /// common case where every numeric attribute was a literal.
    pub numeric_templates: Vec<(NumAttr, String)>,
    pub width: Option<String>,
    pub height: Option<String>,
    pub padding: Option<String>,
    pub align_x: Option<String>,
    pub align_y: Option<String>,
    pub spacing: Option<f32>,
    pub background: Option<String>,
    pub border_radius: Option<f32>,
    pub border_width: Option<f32>,
    pub border_color: Option<String>,
    /// Space-separated stylesheet classes (`class="card centered"`), resolved
    /// against the loaded `.gss` stylesheets during evaluation.
    pub class: Option<String>,
    /// Element id (`id="save"`), matched against `#id { }` GSS selectors during
    /// evaluation. Higher specificity than `class` (applied on top of it, still
    /// under inline attributes). Uniqueness isn't enforced — several nodes may
    /// share an id and all pick up the same `#id` rule.
    pub id: Option<String>,
    /// Font family hint for text-bearing nodes: `mono`/`monospace` selects the
    /// monospaced font; anything else (or `None`) uses the default.
    pub font: Option<String>,
    /// A linear gradient background, overriding `background` when present.
    /// Syntax: `"#RRGGBB #RRGGBB"` (top→bottom) or `"<angle> #a #b [#c ...]"`
    /// where `<angle>` is in degrees.
    pub gradient: Option<String>,
    /// Horizontal text alignment for `Text`: `start`/`center`/`end`.
    pub text_align: Option<String>,
    /// Action dispatched on mouse press (button-down) over this element, which
    /// wraps it in a `mouse_area`. Unlike a `Button`'s click (which fires on
    /// release), press semantics are required for window dragging
    /// (`onPress="window:drag"`). Emitted as an [`crate::EngineMessage::UiClick`].
    pub on_press: Option<String>,
    /// Action dispatched on a double-click over this element (wraps it in a
    /// `mouse_area`). Common for titlebars (`onDoubleClick="window:maximize"`).
    pub on_double_click: Option<String>,
    /// Mouse cursor shown while hovering this element (`cursor="pointer"`,
    /// `cursor="resize-h"`, …). Wraps the element in a `mouse_area` with the
    /// corresponding `mouse::Interaction`. Useful for window resize handles.
    pub cursor: Option<String>,
    /// Cor do rótulo de um `Button` (`textColor`); o `color` do botão é o fundo.
    pub text_color: Option<String>,
    /// Teto de largura/altura (`maxWidth`/`maxHeight`) — envolve o elemento num
    /// `container` que limita, já que `Row`/`Column` não capam o próprio tamanho.
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    /// `hidden`/`oculto` (ou via `.classe { hidden: true }` / `display: none`) —
    /// remove o elemento do layout sem ocupar espaço nem `spacing`. Uso típico:
    /// `@media` esconder cromo em janelas estreitas (ver [`crate::stylesheet::StyleRule::hidden`]).
    pub hidden: Option<bool>,
    /// `disabled`/`desabilitado` — desativa a interação (Button/TextInput/
    /// Checkbox/Toggle deixam de anexar seus handlers `on_press`/`on_input`/
    /// `on_toggle`, então o `Status::Disabled` nativo do iced entra em vigor
    /// sozinho, sem o motor precisar rastrear estado). Ao contrário de
    /// `hidden`, o elemento continua ocupando espaço e renderizando — só perde
    /// a interatividade. Só existe como atributo inline (sem `.classe { }`
    /// equivalente, ao contrário de `hidden`).
    pub disabled: Option<bool>,
    /// Overlays de estilo por pseudo-estado (`.classe:hover { }`,
    /// `:focus`, `:active`, `:disabled`), resolvidos em `eval.rs` a partir
    /// da(s) classe(s) do nó — nunca vêm de atributo inline, sempre `None`
    /// logo após o parse do XML. `None` quando o `.gss` não declara aquele
    /// estado para nenhuma classe do nó (custo zero no caso comum). Aplicados
    /// por `widget.rs` dentro da closure de `Status` do widget correspondente
    /// (ex.: `button::Status::Hovered`); hoje só `Button` e `TextInput`
    /// consultam esses campos — `Select` só usa `hover_style` para a borda.
    pub hover_style: Option<Box<StyleRule>>,
    pub focus_style: Option<Box<StyleRule>>,
    pub active_style: Option<Box<StyleRule>>,
    pub disabled_style: Option<Box<StyleRule>>,
    // Structural directives as attributes (Vue/Angular style)
    pub if_cond: Option<String>,
    pub if_equals: Option<String>,
    pub if_not_equals: Option<String>,
    pub is_else: bool,
    pub for_each: Option<String>,
    pub for_each_var: Option<String>,
    /// Action dispatched (with the new order as a JSON array of `reorderKey`
    /// identities) when a reorderable `for-each`/`ForEach` item is dropped in a
    /// new position. Set on the same node that carries `for_each`/`for_each_var`.
    pub on_reorder: Option<String>,
    /// Field name (within each item's JSON object) used as its stable identity
    /// for reordering — the array itself has no positional index of its own.
    pub reorder_key: Option<String>,
    /// Marks a descendant of a reorderable item as the "grab handle": only it
    /// starts a drag on mouse-press. Doesn't interpolate; a plain marker.
    pub drag_handle: bool,
    /// Internal, evaluation-only fields hydrated by the for-each expansion of a
    /// reorderable list (never set from raw markup): which list (`items` key)
    /// and which item (`reorder_key` value) a node belongs to, used to attach
    /// the hover target (`drag_list`+`drag_item_key`) every item gets, and the
    /// full drag payload (`drag_order`+`drag_on_reorder`) the handle gets.
    pub drag_list: Option<String>,
    pub drag_item_key: Option<String>,
    pub drag_order: Option<Vec<String>>,
    pub drag_on_reorder: Option<String>,
    /// Same, for the `reorderKey` field name — needed by [`crate::GlacierUI`]
    /// to reorder the context's JSON array by identity as the drag moves.
    pub drag_reorder_key: Option<String>,
    /// Binds this input to a [`crate::forms::Form`]'s `FormControl` by name
    /// (Angular's `formControlName`, e.g. `TextInput formControl="email"`). A
    /// `TextInput` with no explicit `value`/`onChange` uses this name for
    /// both, so the input reads/writes the control without repeating the name.
    pub form_control: Option<String>,
    /// Internal, evaluation-only: hydrated by the enclosing `<Form>` (never set
    /// from raw markup) onto every `form_control`-bound descendant — the shared
    /// `"{owner}::{form name}"` prefix used to build this input's stable focus
    /// id and the enclosing form's evaluated `onSubmit` action, so Enter always
    /// fires it (see [`crate::widget::EngineMessage::UiSubmit`]).
    pub form_scope: Option<String>,
    pub form_submit_action: Option<String>,
    /// Internal, evaluation-only: the *name* of the next `form_control` in
    /// document order within the same `<Form>` (`None` on the last one) — Enter
    /// also focuses it, Tab-like, so the user can fill the whole form with the
    /// keyboard alone.
    pub form_next_focus: Option<String>,
}

impl UiNode {
    /// Helper to find a specific attribute case-insensitively
    fn get_attr(node: &Node, keys: &[&str]) -> Option<String> {
        for key in keys {
            if let Some(val) = node.attribute(*key) {
                return Some(val.to_string());
            }
        }
        None
    }

    /// Parse a float attribute. If the value carries a `{...}` placeholder it
    /// can't be parsed yet: the raw string is recorded in `templates` under
    /// `attr` (resolved at eval time) and `None` is returned so the static field
    /// stays empty. A literal value parses as before.
    fn get_attr_num(
        node: &Node,
        keys: &[&str],
        attr: NumAttr,
        templates: &mut Vec<(NumAttr, String)>,
    ) -> Option<f32> {
        match Self::get_attr(node, keys) {
            Some(s) if s.contains('{') => {
                templates.push((attr, s));
                None
            }
            Some(s) => s.parse::<f32>().ok(),
            None => None,
        }
    }

    /// Normalize text content HTML-style: trim the ends and collapse any run of
    /// whitespace (including newlines from multi-line indented source) into a
    /// single space — *except* the non-breaking space (U+00A0, written
    /// `&nbsp;`), which is a hard, literal space. NBSP is never collapsed nor
    /// trimmed, and it absorbs adjacent collapsible whitespace so it stays a
    /// single space; the parser rewrites `&nbsp;` to U+00A0 before parsing (see
    /// [`UiNode::parse_xml`]).
    fn normalize_text(raw: &str) -> String {
        let mut out = String::new();
        let mut pending_space = false; // a run of collapsible whitespace is pending
        for ch in raw.chars() {
            if ch == '\u{00A0}' {
                // Hard space: always emitted, and it consumes any pending
                // collapsible run so `space + nbsp + space` stays one space.
                out.push(' ');
                pending_space = false;
            } else if ch.is_whitespace() {
                pending_space = true;
            } else {
                // Flush the pending run as a single space, unless it would be a
                // leading space (out empty) or double a space already emitted by
                // an adjacent NBSP.
                if pending_space && !out.is_empty() && !out.ends_with(' ') {
                    out.push(' ');
                }
                pending_space = false;
                out.push(ch);
            }
        }
        out
    }

    /// Collect the direct text children of `node`, HTML-style. See
    /// [`UiNode::normalize_text`] for the whitespace/`&nbsp;` rules.
    fn collect_child_text(node: &Node) -> String {
        let raw = node
            .children()
            .filter(|c| c.is_text())
            .filter_map(|c| c.text())
            .collect::<String>();
        Self::normalize_text(&raw)
    }

    /// Helper to parse a bool attribute
    fn get_attr_bool(node: &Node, keys: &[&str]) -> bool {
        Self::get_attr(node, keys)
            .map(|s| s.eq_ignore_ascii_case("true") || s == "1")
            .unwrap_or(false)
    }

    /// Recursively parse a roxmltree Node into UiNode
    pub fn from_node(node: Node) -> Option<Self> {
        if !node.is_element() {
            return None;
        }

        let tag = node.tag_name().name();
        
        // Parse standard layout/style attributes
        let width = Self::get_attr(&node, &["width", "largura", "w"]);
        let height = Self::get_attr(&node, &["height", "altura", "h"]);
        let padding = Self::get_attr(&node, &["padding", "espacamento_interno"]);
        let align_x = Self::get_attr(&node, &["alignX", "align_x", "align-x", "alinhamento_x"]);
        let align_y = Self::get_attr(&node, &["alignY", "align_y", "align-y", "alinhamento_y"]);
        let mut numeric_templates: Vec<(NumAttr, String)> = Vec::new();
        let spacing = Self::get_attr_num(&node, &["spacing", "espacamento"], NumAttr::Spacing, &mut numeric_templates);
        let background = Self::get_attr(&node, &["background", "bg", "fundo"]);
        let border_radius = Self::get_attr_num(&node, &["borderRadius", "border_radius", "border-radius", "raio_borda"], NumAttr::BorderRadius, &mut numeric_templates);
        let border_width = Self::get_attr_num(&node, &["borderWidth", "border_width", "border-width", "largura_borda"], NumAttr::BorderWidth, &mut numeric_templates);
        let border_color = Self::get_attr(&node, &["borderColor", "border_color", "border-color", "cor_borda"]);
        let class = Self::get_attr(&node, &["class", "classe"]);
        let id = Self::get_attr(&node, &["id", "identificador"]);
        let font = Self::get_attr(&node, &["font", "fonte", "fontFamily", "font-family"]);
        let gradient = Self::get_attr(&node, &["gradient", "gradiente"]);
        let text_align = Self::get_attr(&node, &["textAlign", "text_align", "text-align", "alinhamento_texto"]);
        let on_press = Self::get_attr(&node, &["onPress", "on_press", "on-press", "aoPressionar", "ao_pressionar"]);
        let on_double_click = Self::get_attr(&node, &["onDoubleClick", "on_double_click", "on-double-click", "aoClicarDuplo"]);
        let cursor = Self::get_attr(&node, &["cursor", "cursor_", "cursorIcon"]);
        let text_color = Self::get_attr(&node, &["textColor", "text_color", "text-color", "cor_texto"]);
        let max_width = Self::get_attr_num(&node, &["maxWidth", "max_width", "max-width", "largura_max"], NumAttr::MaxWidth, &mut numeric_templates);
        let max_height = Self::get_attr_num(&node, &["maxHeight", "max_height", "max-height", "altura_max"], NumAttr::MaxHeight, &mut numeric_templates);
        let hidden = Self::get_attr(&node, &["hidden", "oculto"])
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1");
        let disabled = Self::get_attr(&node, &["disabled", "desabilitado"])
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1");
        let form_control = Self::get_attr(&node, &["formControl", "form_control", "form-control", "controleForm", "controle_form"]);

        // Structural directives as attributes (Vue/Angular style)
        let if_cond = Self::get_attr(&node, &["if", "se"]);
        let if_equals = Self::get_attr(&node, &["equals", "eq", "igual_a"]);
        let if_not_equals = Self::get_attr(&node, &["notEquals", "not_equals", "ne", "diferente_de"]);
        let is_else = node.has_attribute("else") || node.has_attribute("senao");
        let for_each = Self::get_attr(&node, &["for-each", "forEach", "foreach", "each", "repeat"]);
        let for_each_var = Self::get_attr(&node, &["var", "variavel"]);
        let on_reorder = Self::get_attr(&node, &["onReorder", "on_reorder", "on-reorder", "aoReordenar"]);
        let reorder_key = Self::get_attr(&node, &["reorderKey", "reorder_key", "reorder-key", "chaveReordenar"]);
        let drag_handle = Self::get_attr_bool(&node, &["dragHandle", "drag_handle", "drag-handle", "alcaArraste"]);

        let kind = match tag {
            "Container" | "container" => NodeType::Container,
            "Column" | "column" => NodeType::Column,
            "Row" | "row" => NodeType::Row,
            "Text" | "text" | "Span" | "span" => {
                // Text accepts its content either via the `content` attribute or
                // as a text child (`<Text>lorem ipsum</Text>`). The child wins when
                // both are present. Like HTML, child text is trimmed and any run of
                // whitespace is collapsed to a single space, so long texts can be
                // written across multiple indented lines.
                let child_content = Self::collect_child_text(&node);
                let content = if !child_content.is_empty() {
                    child_content
                } else {
                    Self::get_attr(&node, &["content", "conteudo", "text", "texto"]).unwrap_or_default()
                };
                let size = Self::get_attr_num(&node, &["size", "tamanho"], NumAttr::Size, &mut numeric_templates);
                let bold = Self::get_attr_bool(&node, &["bold", "negrito"]);
                let color = Self::get_attr(&node, &["color", "cor"]);
                NodeType::Text { content, size, bold, color }
            }
            "Button" | "button" | "Botao" | "botao" => {
                let text = Self::get_attr(&node, &["text", "texto", "content", "conteudo"]).unwrap_or_default();
                let on_click = Self::get_attr(&node, &["onClick", "on_click", "on-click", "aoClicar", "ao_clicar"]);
                let navigate_to = Self::get_attr(&node, &["navigateTo", "navigate_to", "navigate-to", "irPara", "ir_para"]);
                let navigate_back = Self::get_attr_bool(&node, &["navigateBack", "navigate_back", "navigate-back", "voltar"]);
                let color = Self::get_attr(&node, &["color", "cor"]);
                NodeType::Button { text, on_click, navigate_to, navigate_back, color }
            }
            "TextInput" | "textinput" | "Input" | "input" | "EntradaTexto" | "entrada_texto" => {
                let placeholder = Self::get_attr(&node, &["placeholder", "dica"]).unwrap_or_default();
                let mut value_var = Self::get_attr(&node, &["value", "valor"]).unwrap_or_default();
                let mut on_change = Self::get_attr(&node, &["onChange", "on_change", "on-change", "aoMudar", "ao_mudar"]).unwrap_or_default();
                // `formControl="username"` without an explicit `value`/`onChange`
                // binds both to the control name, so `Form::sync_to_context`'s
                // `ctx.set(name, ...)` round-trips straight back into this input.
                if let Some(control) = &form_control {
                    if value_var.is_empty() {
                        value_var = control.clone();
                    }
                    if on_change.is_empty() {
                        on_change = control.clone();
                    }
                }
                let secure = Self::get_attr_bool(&node, &["secure", "password", "seguro", "senha"]);
                NodeType::TextInput { placeholder, value_var, on_change, secure }
            }
            "TextArea" | "textarea" | "TextEditor" | "texteditor" | "Editor" | "editor" | "AreaTexto" | "area_texto" => {
                let placeholder = Self::get_attr(&node, &["placeholder", "dica"]).unwrap_or_default();
                let value_var = Self::get_attr(&node, &["value", "valor"]).unwrap_or_default();
                let on_change = Self::get_attr(&node, &["onChange", "on_change", "on-change", "aoMudar", "ao_mudar"]).unwrap_or_default();
                let readonly = Self::get_attr_bool(&node, &["readonly", "read_only", "read-only", "somenteLeitura", "somente_leitura"]);
                NodeType::TextArea { placeholder, value_var, on_change, readonly }
            }
            "Image" | "image" | "Imagem" | "imagem" => {
                let source = Self::get_attr(&node, &["source", "src", "origem", "caminho"]).unwrap_or_default();
                let clip = Self::get_attr(&node, &["clip", "corte"]);
                let clip_circle = clip.map(|s| s.eq_ignore_ascii_case("Circle") || s.eq_ignore_ascii_case("circle")).unwrap_or(false);
                NodeType::Image { source, clip_circle }
            }
            "Svg" | "svg" | "Icon" | "icon" | "Icone" | "icone" => {
                let source = Self::get_attr(&node, &["source", "src", "origem", "caminho"]).unwrap_or_default();
                let color = Self::get_attr(&node, &["color", "cor"]);
                NodeType::Svg { source, color }
            }
            "Scrollable" | "scrollable" | "Scroll" | "scroll" | "Rolagem" | "rolagem" => {
                let direction = Self::get_attr(&node, &["direction", "direcao", "axis", "eixo"])
                    .unwrap_or_else(|| "vertical".to_string());
                NodeType::Scrollable { direction }
            }
            "Checkbox" | "checkbox" | "Check" | "check" => {
                let label = Self::get_attr(&node, &["label", "text", "texto", "rotulo"]).unwrap_or_default();
                let checked_var = Self::get_attr(&node, &["checked", "value", "valor", "marcado"]).unwrap_or_default();
                let on_toggle = Self::get_attr(&node, &["onToggle", "on_toggle", "on-toggle", "onChange", "aoMudar"]).unwrap_or_default();
                NodeType::Checkbox { label, checked_var, on_toggle }
            }
            "Toggle" | "toggle" | "Toggler" | "toggler" | "Switch" | "switch" => {
                let label = Self::get_attr(&node, &["label", "text", "texto", "rotulo"]).unwrap_or_default();
                let checked_var = Self::get_attr(&node, &["checked", "value", "valor", "marcado"]).unwrap_or_default();
                let on_toggle = Self::get_attr(&node, &["onToggle", "on_toggle", "on-toggle", "onChange", "aoMudar"]).unwrap_or_default();
                NodeType::Toggle { label, checked_var, on_toggle }
            }
            "Rule" | "rule" | "Divider" | "divider" | "Divisoria" | "divisoria" => {
                let horizontal = Self::get_attr(&node, &["direction", "direcao", "axis", "eixo"])
                    .map(|s| !s.eq_ignore_ascii_case("vertical") && !s.eq_ignore_ascii_case("v"))
                    .unwrap_or(true);
                NodeType::Rule { horizontal }
            }
            "Select" | "select" | "Dropdown" | "dropdown" | "PickList" | "picklist"
            | "ComboBox" | "combobox" | "Combo" | "combo" | "Seletor" | "seletor" => {
                let options = Self::get_attr(&node, &["options", "items", "itens", "source", "origem", "opcoes"]).unwrap_or_default();
                let value_var = Self::get_attr(&node, &["value", "valor", "selected", "selecionado"]).unwrap_or_default();
                let on_change = Self::get_attr(&node, &["onChange", "on_change", "on-change", "onSelect", "aoMudar", "ao_mudar"]).unwrap_or_default();
                let placeholder = Self::get_attr(&node, &["placeholder", "dica"]).unwrap_or_default();
                let label_field = Self::get_attr(&node, &["labelField", "label_field", "label-field", "labelKey", "campo_rotulo"]).unwrap_or_else(|| "label".to_string());
                let value_field = Self::get_attr(&node, &["valueField", "value_field", "value-field", "valueKey", "campo_valor"]).unwrap_or_else(|| "value".to_string());
                let color = Self::get_attr(&node, &["color", "cor"]);
                NodeType::Select { options, value_var, on_change, placeholder, label_field, value_field, color }
            }
            "Form" | "form" | "Formulario" | "formulario" => {
                let on_submit = Self::get_attr(&node, &["onSubmit", "on_submit", "on-submit", "aoSubmeter", "ao_submeter"]);
                let name = Self::get_attr(&node, &["name", "nome"]);
                NodeType::Form { on_submit, name }
            }
            "Include" | "include" | "Incluir" | "incluir" => {
                let src = Self::get_attr(&node, &["src", "fonte"]).unwrap_or_default();
                // Extract all other attributes as custom parameters
                let mut props = HashMap::new();
                for attr in node.attributes() {
                    let attr_name = attr.name();
                    if attr_name != "src" && attr_name != "fonte" {
                        props.insert(attr_name.to_string(), attr.value().to_string());
                    }
                }
                NodeType::Include { src, props }
            }
            "import" | "Import" | "importar" | "Importar" => {
                let name = Self::get_attr(&node, &["name", "nome", "as"]).unwrap_or_default();
                let from = Self::get_attr(&node, &["from", "de", "src", "path", "caminho"]).unwrap_or_default();
                NodeType::Import { name, from }
            }
            "ForEach" | "foreach" | "For" | "for" => {
                let items = Self::get_attr(&node, &["items", "itens", "source", "origem"]).unwrap_or_default();
                let var = Self::get_attr(&node, &["var", "variavel"]).unwrap_or_default();
                NodeType::ForEach { items, var }
            }
            "If" | "if" | "Se" | "se" => {
                let cond = Self::get_attr(&node, &["cond", "condition", "when", "quando", "condicao"]).unwrap_or_default();
                let equals = Self::get_attr(&node, &["equals", "eq", "igual_a"]);
                let not_equals = Self::get_attr(&node, &["notEquals", "not_equals", "ne", "diferente_de"]);
                NodeType::If { cond, equals, not_equals }
            }
            "Else" | "else" | "Senao" | "senao" => NodeType::Else,
            "link" | "Link" => {
                let rel = Self::get_attr(&node, &["rel", "tipo"])
                    .unwrap_or_else(|| "stylesheet".to_string());
                let href = Self::get_attr(&node, &["href", "src", "from", "caminho"]).unwrap_or_default();
                let name = Self::get_attr(&node, &["as", "name", "nome"]);
                NodeType::Link { rel, href, name }
            }
            "style" | "Style" | "stylesheet" | "Stylesheet" => {
                // `<style href="...">` (or `src`) is an external sheet, equivalent
                // to `<link rel="stylesheet">` (always global). A bodied
                // `<style>...</style>` is inline `.gss` source, global by default
                // unless marked `scoped="true"`.
                if let Some(href) = Self::get_attr(&node, &["href", "src", "from", "caminho"]) {
                    NodeType::Link { rel: "stylesheet".to_string(), href, name: None }
                } else {
                    let css = node
                        .children()
                        .filter(|c| c.is_text())
                        .filter_map(|c| c.text())
                        .collect::<String>();
                    let scoped = Self::get_attr_bool(&node, &["scoped", "escopado"]);
                    NodeType::Style { css, scoped }
                }
            }
            _ => {
                // Any unknown tag is treated as a reference to another component
                // by its own name (e.g. <PerfilCard nome="..." />).
                // All attributes are forwarded as props.
                let mut props = HashMap::new();
                for attr in node.attributes() {
                    props.insert(attr.name().to_string(), attr.value().to_string());
                }
                NodeType::Component {
                    name: tag.to_string(),
                    props,
                }
            }
        };

        // Recursively parse children. Bare text nodes aren't elements, so
        // `from_node` skips them; instead we wrap any non-empty loose text in
        // an implicit `Text` node (HTML-style), collapsing whitespace runs the
        // same way `collect_child_text` does. Nodes that already consume their
        // text child (`Text`/`Span`) are excluded so the content isn't
        // duplicated.
        let wrap_loose_text = !matches!(kind, NodeType::Text { .. });
        let mut children = Vec::new();
        for child in node.children() {
            if let Some(child_node) = Self::from_node(child) {
                children.push(child_node);
            } else if wrap_loose_text && child.is_text() {
                let text = Self::normalize_text(child.text().unwrap_or_default());
                if !text.is_empty() {
                    children.push(empty_node(
                        NodeType::Text { content: text, size: None, bold: false, color: None },
                        Vec::new(),
                    ));
                }
            }
        }

        Some(Self {
            kind,
            children,
            numeric_templates,
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
            id,
            font,
            gradient,
            text_align,
            on_press,
            on_double_click,
            cursor,
            text_color,
            max_width,
            max_height,
            hidden,
            disabled,
            hover_style: None,
            focus_style: None,
            active_style: None,
            disabled_style: None,
            if_cond,
            if_equals,
            if_not_equals,
            is_else,
            for_each,
            for_each_var,
            on_reorder,
            reorder_key,
            drag_handle,
            drag_list: None,
            drag_item_key: None,
            drag_order: None,
            drag_on_reorder: None,
            drag_reorder_key: None,
            form_control,
            form_scope: None,
            form_submit_action: None,
            form_next_focus: None,
        })
    }

    /// Parse a full XML string into UiNode.
    ///
    /// A file may declare `<import name="..." from="..." />` or
    /// `<link rel="stylesheet" href="..." />` at the top level, before its
    /// actual root element. To allow these sibling declarations the content is
    /// wrapped in a synthetic root before parsing; the declarations are then
    /// attached to the real root as children (they are stripped before
    /// rendering, so they have no visual effect but remain discoverable).
    pub fn parse_xml(xml: &str) -> Result<Self, String> {
        // `&nbsp;` isn't a predefined XML entity, so roxmltree would reject it.
        // Rewrite it to a literal non-breaking space (U+00A0) up front; the text
        // normalizer then preserves it as a hard space (see `normalize_text`).
        let xml = xml.replace("&nbsp;", "\u{00A0}");
        let wrapped = format!("<__glacier_fragment__>{}</__glacier_fragment__>", xml);
        let doc = roxmltree::Document::parse(&wrapped).map_err(|e| e.to_string())?;
        let fragment = doc.root_element();

        let mut decls = Vec::new();
        let mut roots: Vec<Self> = Vec::new();
        for child in fragment.children() {
            if let Some(node) = Self::from_node(child) {
                if matches!(node.kind, NodeType::Import { .. } | NodeType::Link { .. } | NodeType::Style { .. }) {
                    decls.push(node);
                } else {
                    roots.push(node);
                }
            }
        }

        // Multiple top-level layout nodes become a `Fragment` (their siblings
        // are spliced into the parent at eval time) instead of silently keeping
        // only the first — so a component template can be an `if`/`else` pair
        // (or any list of siblings) with no wrapper node. A single root is kept
        // as-is for backwards compatibility. Declarations ride along as
        // children (they're stripped during evaluation) so `load_imports` /
        // `process_links` still find them.
        let mut root = match roots.len() {
            0 => return Err("No root element found".to_string()),
            1 => roots.pop().expect("len checked"),
            _ => empty_node(NodeType::Fragment, roots),
        };
        root.children.extend(decls);
        Ok(root)
    }
}

#[cfg(test)]
mod loose_text_tests {
    use super::*;

    fn text_of(node: &UiNode) -> Option<&str> {
        match &node.kind {
            NodeType::Text { content, .. } => Some(content.as_str()),
            _ => None,
        }
    }

    // Bare text inside a layout node is wrapped in an implicit `Text` child,
    // with whitespace collapsed HTML-style.
    #[test]
    fn loose_text_becomes_implicit_text() {
        let root = UiNode::parse_xml("<Column>  ola   mundo  </Column>").unwrap();
        assert!(matches!(root.kind, NodeType::Column));
        assert_eq!(root.children.len(), 1);
        assert_eq!(text_of(&root.children[0]), Some("ola mundo"));
    }

    // Text interleaved with elements yields one implicit `Text` per run.
    #[test]
    fn loose_text_interleaved_with_elements() {
        let root = UiNode::parse_xml("<Row>antes<Text content=\"meio\"/>depois</Row>").unwrap();
        let contents: Vec<_> = root.children.iter().filter_map(text_of).collect();
        assert_eq!(contents, vec!["antes", "meio", "depois"]);
    }

    // A real `Text`/`Span` still consumes its own text child; no duplicate
    // implicit node is inserted alongside the parsed `content`.
    #[test]
    fn text_node_does_not_double_wrap() {
        let root = UiNode::parse_xml("<Text>ola</Text>").unwrap();
        assert_eq!(text_of(&root), Some("ola"));
        assert!(root.children.is_empty(), "text child was consumed, not re-wrapped");
    }

    // Whitespace-only text between elements is dropped, not turned into an
    // empty `Text`.
    #[test]
    fn whitespace_only_text_is_ignored() {
        let root = UiNode::parse_xml("<Column>\n  <Text content=\"a\"/>\n</Column>").unwrap();
        assert_eq!(root.children.len(), 1);
        assert_eq!(text_of(&root.children[0]), Some("a"));
    }

    // `&nbsp;` becomes a literal space that survives trimming and collapsing.
    #[test]
    fn nbsp_is_a_hard_space_and_not_trimmed() {
        // Leading/trailing NBSP is preserved (not trimmed).
        let root = UiNode::parse_xml("<Text>&nbsp;ola&nbsp;</Text>").unwrap();
        assert_eq!(text_of(&root), Some(" ola "));

        // Multiple NBSP are all preserved as spaces (hard, non-collapsing).
        let root = UiNode::parse_xml("<Text>a&nbsp;&nbsp;&nbsp;b</Text>").unwrap();
        assert_eq!(text_of(&root), Some("a   b"));
    }

    // A NBSP absorbs adjacent collapsible whitespace so it stays a single space.
    #[test]
    fn nbsp_absorbs_adjacent_whitespace() {
        let root = UiNode::parse_xml("<Text>a &nbsp; b</Text>").unwrap();
        assert_eq!(text_of(&root), Some("a b"));
    }

    // `&nbsp;` also works in loose text wrapped into an implicit `Text`.
    #[test]
    fn nbsp_in_loose_text() {
        let root = UiNode::parse_xml("<Column>&nbsp;ola&nbsp;mundo&nbsp;</Column>").unwrap();
        assert_eq!(root.children.len(), 1);
        assert_eq!(text_of(&root.children[0]), Some(" ola mundo "));
    }
}

/// A bare [`UiNode`] of `kind` with the given `children` and every optional
/// field defaulted — used for synthetic nodes the parser inserts (e.g. the
/// `Fragment` wrapping multiple top-level nodes).
pub(crate) fn empty_node(kind: NodeType, children: Vec<UiNode>) -> UiNode {
    UiNode {
        kind,
        children,
        numeric_templates: Vec::new(),
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
        id: None,
        font: None,
        gradient: None,
        text_align: None,
        on_press: None,
        on_double_click: None,
        cursor: None,
        text_color: None,
        max_width: None,
        max_height: None,
        hidden: None,
        disabled: None,
        hover_style: None,
        focus_style: None,
        active_style: None,
        disabled_style: None,
        if_cond: None,
        if_equals: None,
        if_not_equals: None,
        is_else: false,
        for_each: None,
        for_each_var: None,
        on_reorder: None,
        reorder_key: None,
        drag_handle: false,
        drag_list: None,
        drag_item_key: None,
        drag_order: None,
        drag_on_reorder: None,
        drag_reorder_key: None,
        form_control: None,
        form_scope: None,
        form_submit_action: None,
        form_next_focus: None,
    }
}

