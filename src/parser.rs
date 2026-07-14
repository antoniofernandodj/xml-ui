use std::collections::HashMap;
use roxmltree::Node;
use crate::error::{Diagnostic, GlacierError, Result};
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
        /// Linha (1-based) do `<style>` no arquivo que o declarou. É o
        /// deslocamento que traduz a linha de um erro **dentro do `.gss`
        /// inline** para a linha do arquivo XML onde o autor a escreveu — sem
        /// isso, um erro de GSS num `<style>` na linha 200 seria reportado como
        /// "linha 3" (a 3ª linha do corpo), que não ajuda ninguém. Ver
        /// [`crate::stylesheet::parse_gss_in`].
        line: u32,
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
    /// Texto exibido num balão flutuante ao pairar o mouse sobre o elemento
    /// (`tooltip="..."`/`title="..."`), via `iced::widget::Tooltip`. Só
    /// atributo inline (sem `.classe { }` equivalente — é conteúdo, não
    /// estilo). Útil sobretudo em ícones sem rótulo visível (ex.: a sidebar
    /// colapsada de rustploy-gui).
    pub tooltip: Option<String>,
    /// Lado do balão (`tooltipPosition="right"`, padrão): `top`/`bottom`/
    /// `left`/`right`/`follow` (segue o cursor). Ignorado sem `tooltip`.
    pub tooltip_position: Option<String>,
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
        let tooltip = Self::get_attr(&node, &["tooltip", "title", "dica"]);
        let tooltip_position = Self::get_attr(&node, &["tooltipPosition", "tooltip_position", "tooltip-position"]);
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
                    // O corpo veio blindado em CDATA (ver `protect_style_bodies`),
                    // e o roxmltree entrega CDATA como nó de texto comum — então
                    // `.gss` com `<`, `&` ou tags citadas em comentário chega aqui
                    // verbatim, sem nunca ter sido lido como XML.
                    let css = node
                        .children()
                        .filter(|c| c.is_text())
                        .filter_map(|c| c.text())
                        .collect::<String>();
                    let scoped = Self::get_attr_bool(&node, &["scoped", "escopado"]);
                    // Linha do `<style>` no arquivo, para posicionar erros do
                    // `.gss` inline (o pré-processamento preserva linhas).
                    let line = node.document().text_pos_at(node.range().start).row;
                    NodeType::Style { css, scoped, line }
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
            tooltip,
            tooltip_position,
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

    /// Parse a full XML string into UiNode, sem saber de que arquivo veio — os
    /// erros saem posicionados (linha/coluna) mas sem caminho. Prefira
    /// [`UiNode::parse_xml_in`] quando houver um arquivo a citar.
    pub fn parse_xml(xml: &str) -> Result<Self> {
        Self::parse_xml_in(xml, None)
    }

    /// Parse a full XML string into UiNode, citando `file` nos erros.
    ///
    /// A file may declare `<import name="..." from="..." />` or
    /// `<link rel="stylesheet" href="..." />` at the top level, before its
    /// actual root element. To allow these sibling declarations the content is
    /// wrapped in a synthetic root before parsing; the declarations are then
    /// attached to the real root as children (they are stripped before
    /// rendering, so they have no visual effect but remain discoverable).
    ///
    /// **Toda transformação feita aqui antes do parse preserva a contagem de
    /// linhas**, para o `line` que o roxmltree reporta ser o `line` do arquivo
    /// que o autor escreveu (ver [`crate::error::Diagnostic`]). O embrulho na
    /// raiz sintética fica na linha 1 (a coluna é descontada depois), e o corpo
    /// dos `<style>` é blindado com CDATA — que não introduz quebra de linha.
    pub fn parse_xml_in(xml: &str, file: Option<&str>) -> Result<Self> {
        Self::parse_xml_with_source(xml, xml, file)
    }

    /// Igual a [`UiNode::parse_xml_in`], mas recorta o trecho ofensor dos erros
    /// de `source` em vez de `xml`.
    ///
    /// Os dois diferem porque o motor **pré-processa** o markup antes de
    /// parseá-lo (tira o `<script>`, reescreve `else` como `else=""`): `xml` é o
    /// resultado dessas passadas — o texto que o roxmltree de fato viu, e a que
    /// as posições se referem — enquanto `source` é o arquivo como o autor o
    /// escreveu, que é o que ele espera ver de volta na mensagem de erro. Como
    /// as passadas preservam a contagem de linhas, a linha serve para os dois.
    pub fn parse_xml_with_source(xml: &str, source: &str, file: Option<&str>) -> Result<Self> {
        // `&nbsp;` isn't a predefined XML entity, so roxmltree would reject it.
        // Rewrite it to a literal non-breaking space (U+00A0) up front; the text
        // normalizer then preserves it as a hard space (see `normalize_text`).
        let prepared = protect_style_bodies(&xml.replace("&nbsp;", "\u{00A0}"));
        let wrapped = format!("{FRAGMENT_OPEN}{prepared}</__glacier_fragment__>");
        let doc = roxmltree::Document::parse(&wrapped)
            .map_err(|e| xml_error(e, source, file))?;
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
            0 => {
                let mut d = Diagnostic::new(1, 1, "o template não tem nenhum elemento raiz")
                    .with_hint("um template precisa de ao menos um nó de layout (ex.: <Column>…</Column>); \
                                só <import>/<link>/<style> não basta");
                if let Some(f) = file {
                    d = d.in_file(f, source);
                }
                return Err(GlacierError::Xml(Box::new(d)));
            }
            1 => roots.pop().expect("len checked"),
            _ => empty_node(NodeType::Fragment, roots),
        };
        root.children.extend(decls);
        Ok(root)
    }
}

/// Raiz sintética que envolve o documento para permitir declarações irmãs da
/// raiz real (`<import>`/`<link>`/`<style>` antes do nó de layout). Fica **na
/// linha 1**, então só desloca a *coluna* dessa linha — o que [`xml_error`]
/// desconta ao traduzir a posição de volta para o arquivo original.
const FRAGMENT_OPEN: &str = "<__glacier_fragment__>";

/// Traduz um erro do roxmltree — cuja posição se refere ao documento
/// **preprocessado** — num [`Diagnostic`] posicionado no arquivo **original**,
/// com o trecho ofensor e, quando a causa é uma pegadinha conhecida, uma dica.
fn xml_error(err: roxmltree::Error, original: &str, file: Option<&str>) -> GlacierError {
    let pos = err.pos();
    // Só a linha 1 carrega o prefixo da raiz sintética; nas demais a coluna já
    // é a do arquivo. `saturating_sub` protege contra um erro reportado dentro
    // do próprio prefixo (não deveria acontecer, mas não vale um panic).
    let col = if pos.row == 1 {
        pos.col.saturating_sub(FRAGMENT_OPEN.len() as u32).max(1)
    } else {
        pos.col
    };

    let mut d = Diagnostic::new(pos.row, col, xml_message(&err));
    d = match file {
        Some(f) => d.in_file(f, original),
        None => d.with_source(original),
    };
    if let Some(hint) = xml_hint(&err) {
        d = d.with_hint(hint);
    }
    GlacierError::Xml(Box::new(d))
}

/// A mensagem do erro, em português, para os casos que o motor sabe nomear —
/// e o texto do roxmltree (em inglês) para o resto, que é raro. O `Display` do
/// roxmltree termina em " at linha:coluna"; esse sufixo é cortado porque a
/// posição agora é responsabilidade do [`Diagnostic`] (e a dele se referiria ao
/// documento pré-processado, não ao arquivo do autor).
fn xml_message(err: &roxmltree::Error) -> String {
    use roxmltree::Error as E;
    match err {
        E::UnexpectedCloseTag(expected, actual, _) => {
            format!("esperava a tag de fechamento '{expected}', encontrei '{actual}'")
        }
        E::UnknownEntityReference(name, _) => format!("entidade '&{name};' desconhecida"),
        E::MalformedEntityReference(_) => "referência de entidade mal formada".to_string(),
        E::DuplicatedAttribute(name, _) => format!("atributo '{name}' repetido na mesma tag"),
        E::UnclosedRootNode => "o elemento raiz nunca é fechado".to_string(),
        E::UnexpectedEndOfStream => "o documento termina no meio de uma tag".to_string(),
        E::InvalidName(..) => "nome de tag ou atributo inválido".to_string(),
        E::InvalidComment(_) => "comentário XML mal formado".to_string(),
        other => {
            let raw = other.to_string();
            raw.rsplit_once(" at ").map_or(raw.as_str(), |(head, _)| head).to_string()
        }
    }
}

/// A dica ("como saio disso") para os erros de XML que o motor sabe reconhecer.
/// São exatamente as pegadinhas que já custaram tempo de depuração de verdade —
/// um XML mal formado por um `&` solto ou por uma tag de fechamento trocada é o
/// que 90% dos erros de template são na prática.
fn xml_hint(err: &roxmltree::Error) -> Option<&'static str> {
    use roxmltree::Error as E;
    Some(match err {
        E::UnexpectedCloseTag { .. } => {
            "tag de fechamento trocada ou aninhamento errado — confira a grafia \
             (maiúsculas contam: <Column> fecha com </Column>) e se nenhuma tag \
             ficou aberta antes desta"
        }
        E::UnknownEntityReference(..) | E::MalformedEntityReference(..) => {
            "o XML só conhece &amp; &lt; &gt; &quot; &apos; (e &nbsp;, que o glacier \
             traduz) — para um `&` literal num atributo, escreva `&amp;`"
        }
        E::InvalidName(..) | E::UnknownToken(..) => {
            "nome de tag/atributo inválido — um `<` solto no texto precisa virar `&lt;`"
        }
        E::UnclosedRootNode | E::UnexpectedEndOfStream => {
            "tag aberta e nunca fechada — todo elemento sem filhos precisa terminar em `/>`"
        }
        E::InvalidComment(..) => {
            "comentário XML mal formado — `--` não pode aparecer dentro de `<!-- ... -->`"
        }
        E::DuplicatedAttribute(..) => "o mesmo atributo aparece duas vezes na tag",
        _ => return None,
    })
}

/// Blinda o corpo de cada `<style>…</style>` envolvendo-o em `<![CDATA[…]]>`,
/// para que o parser de XML **nunca** olhe dentro dele.
///
/// Sem isso, o corpo do `<style>` é XML como qualquer outro texto — e um `<` no
/// CSS, ou uma tag citada num comentário do CSS (`/* .card vira <Text> */`),
/// vira um elemento de verdade aos olhos do roxmltree. O erro que sai daí é
/// dos piores possíveis: aponta o `</style>` (linha errada) e reclama de uma
/// tag que o autor nunca abriu ("expected 'Text' tag, not 'style'"). Como o
/// corpo do `<style>` não é XML — é `.gss`, uma outra linguagem — a correção
/// certa é tirá-lo do alcance do parser, não pedir ao autor que escape o CSS.
///
/// Não introduz quebras de linha (os marcadores são inline), então a contagem
/// de linhas do documento — e portanto toda posição de erro — fica intacta.
/// Blocos já em CDATA, ou cujo corpo contenha o terminador `]]>`, são deixados
/// como estão (não há como aninhar CDATA; o caso não ocorre em `.gss` real).
fn protect_style_bodies(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len() + 32);
    let mut rest = xml;

    loop {
        // Pula comentários inteiros: um `<style>` citado dentro de `<!-- -->`
        // não é um bloco de verdade e não deve ser tocado.
        let comment = rest.find("<!--");
        let style = find_style_open(rest);

        let Some(open) = style else {
            out.push_str(rest);
            return out;
        };
        if let Some(c) = comment {
            if c < open {
                let end = rest[c..].find("-->").map(|e| c + e + 3).unwrap_or(rest.len());
                out.push_str(&rest[..end]);
                rest = &rest[end..];
                continue;
            }
        }

        // `<style ...>` — o corpo começa depois do `>` da tag de abertura. Uma
        // tag vazia (`<style href="..."/>`) não tem corpo a proteger.
        let Some(gt) = rest[open..].find('>').map(|i| open + i) else {
            out.push_str(rest);
            return out;
        };
        let body_start = gt + 1;
        if rest[open..gt].ends_with('/') {
            out.push_str(&rest[..body_start]);
            rest = &rest[body_start..];
            continue;
        }
        let Some(close) = find_style_close(&rest[body_start..]).map(|i| body_start + i) else {
            out.push_str(rest);
            return out;
        };

        let body = &rest[body_start..close];
        out.push_str(&rest[..body_start]);
        if body.contains("]]>") || body.trim_start().starts_with("<![CDATA[") {
            out.push_str(body);
        } else {
            out.push_str("<![CDATA[");
            out.push_str(body);
            out.push_str("]]>");
        }
        rest = &rest[close..];
    }
}

/// Índice do próximo `<style`/`<Style` que abre uma tag de verdade (seguido de
/// espaço, `>` ou `/`, para não casar um `<styles>` qualquer).
fn find_style_open(s: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = s[from..].find('<').map(|i| from + i) {
        let tail = &s[i + 1..];
        let name_len = if tail.len() >= 5 && tail[..5].eq_ignore_ascii_case("style") {
            5
        } else {
            from = i + 1;
            continue;
        };
        match tail.as_bytes().get(name_len) {
            Some(b' ' | b'\t' | b'\n' | b'\r' | b'>' | b'/') => return Some(i),
            _ => from = i + 1,
        }
    }
    None
}

/// Índice do `</style>` que fecha o bloco corrente (case-insensitive).
fn find_style_close(s: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = s[from..].find("</").map(|i| from + i) {
        let tail = &s[i + 2..];
        if tail.len() >= 5 && tail[..5].eq_ignore_ascii_case("style") {
            return Some(i);
        }
        from = i + 2;
    }
    None
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;

    // A regressão que motivou `protect_style_bodies`: uma tag citada num
    // comentário do CSS fazia o XML parser reclamar de uma tag que o autor
    // nunca abriu, apontando o `</style>`. Agora o corpo do <style> é opaco.
    #[test]
    fn tag_em_comentario_de_css_nao_quebra_o_parse() {
        let xml = "<Column>\n  <style>\n    /* o card vira <Text> aqui */\n    .card { padding: 8; }\n  </style>\n  <Text content=\"oi\" />\n</Column>";
        let root = UiNode::parse_xml(xml).expect("o corpo do <style> não é XML");
        let css = root.children.iter().find_map(|c| match &c.kind {
            NodeType::Style { css, .. } => Some(css.as_str()),
            _ => None,
        });
        assert!(css.is_some_and(|c| c.contains(".card") && c.contains("<Text>")));
    }

    // Um `<` literal no CSS (seletor imaginário, expressão) idem: é CSS, não XML.
    #[test]
    fn menor_que_literal_no_css_nao_quebra_o_parse() {
        let xml = "<Column><style>.a { width: 10; } /* a < b */</style><Text content=\"x\"/></Column>";
        assert!(UiNode::parse_xml(xml).is_ok());
    }

    // `<style href="...">` (auto-fechada, sem corpo) continua virando um Link.
    #[test]
    fn style_sem_corpo_segue_virando_link() {
        let xml = "<Column><style href=\"a.gss\" /><Text content=\"x\"/></Column>";
        let root = UiNode::parse_xml(xml).unwrap();
        assert!(root.children.iter().any(|c| matches!(&c.kind, NodeType::Link { href, .. } if href == "a.gss")));
    }

    // O erro precisa apontar o arquivo, a linha REAL e a coluna, e trazer a
    // linha ofensora — é o contrato do ponto "falhar apontando o dedo certo".
    #[test]
    fn erro_de_xml_traz_arquivo_linha_e_trecho() {
        let xml = "<Column>\n  <Text content=\"a\" />\n</Colunm>\n";
        let err = UiNode::parse_xml_in(xml, Some("views/home.xml")).unwrap_err();
        let d = err.diagnostic().expect("erro de XML tem diagnóstico");
        assert_eq!(d.file.as_deref(), Some("views/home.xml"));
        assert_eq!(d.line, 3, "a linha do </Colunm>");
        assert!(d.snippet.as_deref().is_some_and(|s| s.contains("Colunm")));
        assert!(d.hint.is_some(), "tag de fechamento trocada tem dica");
    }

    // Um erro na PRIMEIRA linha não pode herdar a coluna da raiz sintética
    // (22 chars) — antes o caret apontava para o meio do nada.
    #[test]
    fn coluna_da_linha_1_desconta_a_raiz_sintetica() {
        let err = UiNode::parse_xml_in("<Column></Row>", Some("t.xml")).unwrap_err();
        let d = err.diagnostic().unwrap();
        assert_eq!(d.line, 1);
        assert!(d.col <= 14, "coluna {} saiu do tamanho da linha", d.col);
    }

    // Template sem nó de layout tem mensagem própria (e dica), não um erro
    // genérico de XML.
    #[test]
    fn template_sem_raiz_tem_mensagem_propria() {
        let err = UiNode::parse_xml_in("<link rel=\"stylesheet\" href=\"a.gss\" />", Some("t.xml"))
            .unwrap_err();
        let d = err.diagnostic().unwrap();
        assert!(d.message.contains("raiz"), "{}", d.message);
        assert!(d.hint.is_some());
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
        tooltip: None,
        tooltip_position: None,
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

