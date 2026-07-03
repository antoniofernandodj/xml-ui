use std::collections::HashMap;
use roxmltree::Node;

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
    /// file at `from`, e.g. `<import name="PerfilCard" from="templates/perfil_card.xml" />`.
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
    /// - `stylesheet` (default): an `.gss` sheet, scoped to the declaring
    ///   component's subtree (on top of any global sheets);
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
    /// An inline, component-scoped stylesheet, e.g.
    /// `<style>.card { padding: 16; }</style>`. The body is `.gss` source (the
    /// same grammar as a linked `.gss` file); it is parsed and applied only
    /// inside the declaring component's subtree, layered on top of the global
    /// sheets exactly like a `<link rel="stylesheet">`. Processed at
    /// registration time and stripped before rendering.
    Style {
        css: String,
    },
    /// A transparent grouping node: renders its children inline into the parent,
    /// adding no layout box of its own. Produced by [`crate::parse_kdl`] when a
    /// template has more than one top-level node (so a component template can be
    /// a "fragment" of siblings — e.g. an `if`/`else` pair — without a wrapper),
    /// and also writable explicitly as `Fragment { … }`. During evaluation
    /// (`expand_children`) a `Fragment`'s children are spliced into the
    /// surrounding list, so it normally never reaches rendering; `render_node`
    /// falls back to stacking them in a `Column` if one ever does (e.g. a
    /// multi-root screen root).
    Fragment,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiNode {
    pub kind: NodeType,
    pub children: Vec<UiNode>,
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

    /// Helper to parse a float attribute
    fn get_attr_f32(node: &Node, keys: &[&str]) -> Option<f32> {
        Self::get_attr(node, keys).and_then(|s| s.parse::<f32>().ok())
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
        let spacing = Self::get_attr_f32(&node, &["spacing", "espacamento"]);
        let background = Self::get_attr(&node, &["background", "bg", "fundo"]);
        let border_radius = Self::get_attr_f32(&node, &["borderRadius", "border_radius", "border-radius", "raio_borda"]);
        let border_width = Self::get_attr_f32(&node, &["borderWidth", "border_width", "border-width", "largura_borda"]);
        let border_color = Self::get_attr(&node, &["borderColor", "border_color", "border-color", "cor_borda"]);
        let class = Self::get_attr(&node, &["class", "classe"]);
        let font = Self::get_attr(&node, &["font", "fonte", "fontFamily", "font-family"]);
        let gradient = Self::get_attr(&node, &["gradient", "gradiente"]);
        let text_align = Self::get_attr(&node, &["textAlign", "text_align", "text-align", "alinhamento_texto"]);
        let on_press = Self::get_attr(&node, &["onPress", "on_press", "on-press", "aoPressionar", "ao_pressionar"]);
        let on_double_click = Self::get_attr(&node, &["onDoubleClick", "on_double_click", "on-double-click", "aoClicarDuplo"]);
        let cursor = Self::get_attr(&node, &["cursor", "cursor_", "cursorIcon"]);
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
            "Text" | "text" => {
                let content = Self::get_attr(&node, &["content", "conteudo", "text", "texto"]).unwrap_or_default();
                let size = Self::get_attr_f32(&node, &["size", "tamanho"]);
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
                NodeType::TextArea { placeholder, value_var, on_change }
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
                // to `<link rel="stylesheet">`. A bodied `<style>...</style>` is an
                // inline, component-scoped sheet whose text content is `.gss`.
                if let Some(href) = Self::get_attr(&node, &["href", "src", "from", "caminho"]) {
                    NodeType::Link { rel: "stylesheet".to_string(), href, name: None }
                } else {
                    let css = node
                        .children()
                        .filter(|c| c.is_text())
                        .filter_map(|c| c.text())
                        .collect::<String>();
                    NodeType::Style { css }
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

        // Recursively parse children
        let mut children = Vec::new();
        for child in node.children() {
            if let Some(child_node) = Self::from_node(child) {
                children.push(child_node);
            }
        }

        Some(Self {
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
        // `process_links` still find them. Mirrors the KDL parser exactly.
        let mut root = match roots.len() {
            0 => return Err("No root element found".to_string()),
            1 => roots.pop().expect("len checked"),
            _ => empty_node(NodeType::Fragment, roots),
        };
        root.children.extend(decls);
        Ok(root)
    }
}

/// A bare [`UiNode`] of `kind` with the given `children` and every optional
/// field defaulted — used for synthetic nodes the parser inserts (e.g. the
/// `Fragment` wrapping multiple top-level nodes).
pub(crate) fn empty_node(kind: NodeType, children: Vec<UiNode>) -> UiNode {
    UiNode {
        kind,
        children,
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

