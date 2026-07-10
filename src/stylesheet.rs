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
    /// `hidden: true` (ou `display: none`) — remove o elemento do layout, como
    /// `display:none` do CSS: não ocupa espaço nem gera `spacing` entre irmãos.
    /// Pensado sobretudo para `@media` esconder cromo de baixa prioridade
    /// (busca, status, cards) em janelas estreitas, evitando que Rows estourem
    /// e sobreponham botões/textos (o motor não reflui Row↔Column).
    pub hidden: Option<bool>,
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
        if other.hidden.is_some() { self.hidden = other.hidden; }
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

/// Um pseudo-estado suportado em seletores `.classe:estado { }` — a única
/// outra quebra (com `:root`) da regra "seletor = classe". Mapeado, na camada
/// `widget.rs`, para o `Status` nativo de cada widget do iced (ex.:
/// `button::Status::Hovered`), então o motor não precisa rastrear hover/foco
/// manualmente — reaproveita a máquina de estado que o iced já tem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PseudoState {
    Hover,
    Focus,
    Active,
    Disabled,
}

impl PseudoState {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "hover" => Some(Self::Hover),
            "focus" => Some(Self::Focus),
            // `pressed` aceito como alias de `active` (nome que o iced usa
            // para o mesmo conceito em `button::Status::Pressed`).
            "active" | "pressed" => Some(Self::Active),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

/// Overlays de estilo por pseudo-estado para uma lista de classes, já
/// mesclados através de classes/sheets/`@media`/`var()` (mesma lógica de
/// [`resolve_classes`]) — mas sem interpolação de contexto (feita depois, no
/// `eval.rs`, como o resto dos campos). Cada campo fica em seu default
/// (`StyleRule::default()`, isto é "nada declarado") quando o `.gss` não tem
/// um bloco `:estado` para nenhuma das classes. Aplicado por `widget.rs`
/// dentro da closure de `Status` do widget correspondente.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StateStyles {
    pub hover: StyleRule,
    pub focus: StyleRule,
    pub active: StyleRule,
    pub disabled: StyleRule,
}

impl StateStyles {
    /// Sobrepõe cada estado de `other` sobre o de `self` (campo a campo, via
    /// [`StyleRule::merge_from`]). Usado para dobrar o overlay de tag/underlay
    /// sob os de classe/id.
    pub fn merge_from(&mut self, other: &StateStyles) {
        self.hover.merge_from(&other.hover);
        self.focus.merge_from(&other.focus);
        self.active.merge_from(&other.active);
        self.disabled.merge_from(&other.disabled);
    }

    fn get_mut(&mut self, state: PseudoState) -> &mut StyleRule {
        match state {
            PseudoState::Hover => &mut self.hover,
            PseudoState::Focus => &mut self.focus,
            PseudoState::Active => &mut self.active,
            PseudoState::Disabled => &mut self.disabled,
        }
    }
}

/// Condição de um bloco `@media` — features `min/max-width` e `min/max-height`
/// (em px lógicos), combinadas por AND (como no CSS). `None` = sem restrição.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaCondition {
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
}

impl MediaCondition {
    /// `true` se o viewport `(w, h)` satisfaz todas as features declaradas.
    pub fn matches(&self, w: f32, h: f32) -> bool {
        self.min_width.is_none_or(|v| w >= v)
            && self.max_width.is_none_or(|v| w <= v)
            && self.min_height.is_none_or(|v| h >= v)
            && self.max_height.is_none_or(|v| h <= v)
    }
}

/// Um bloco `@media (cond) { .a {…} .b {…} }`: as regras valem só quando a
/// condição casa com o viewport atual, aplicadas POR CIMA das regras base.
#[derive(Debug, Clone)]
pub struct MediaQuery {
    pub condition: MediaCondition,
    pub rules: HashMap<String, StyleRule>,
    /// `.classe:estado { }` declarados dentro deste bloco `@media` (ver
    /// [`StyleSheet::states`] — mesma forma, com escopo de viewport).
    pub states: HashMap<String, HashMap<PseudoState, StyleRule>>,
    /// `#id { }` declarados dentro deste `@media` (ver [`StyleSheet::ids`]).
    pub ids: HashMap<String, StyleRule>,
    /// `#id:estado { }` declarados dentro deste `@media` (ver
    /// [`StyleSheet::id_states`]).
    pub id_states: HashMap<String, HashMap<PseudoState, StyleRule>>,
    /// `Tag { }` declarados dentro deste `@media` (ver [`StyleSheet::tags`]).
    pub tags: HashMap<String, StyleRule>,
    /// `Tag:estado { }` declarados dentro deste `@media` (ver
    /// [`StyleSheet::tag_states`]).
    pub tag_states: HashMap<String, HashMap<PseudoState, StyleRule>>,
}

/// A parsed `.gss` document: a map from class name (without the leading `.`)
/// to its [`StyleRule`], plus the design tokens declared in a `:root { --x }`
/// block (referenced elsewhere as `var(--x)`), plus any `@media` blocks.
#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    pub rules: HashMap<String, StyleRule>,
    /// Variáveis/design tokens de `:root { --nome: valor; }`, sem o `--`… não:
    /// a chave guarda o `--nome` completo (como aparece em `var(--nome)`).
    pub variables: HashMap<String, String>,
    /// Blocos `@media` — regras condicionais ao viewport (ver [`MediaQuery`]).
    pub media: Vec<MediaQuery>,
    /// `.classe:estado { }` (`:hover`/`:focus`/`:active`/`:disabled`), por
    /// classe e depois por estado. Resolvidos separadamente da base via
    /// [`resolve_state_classes`] — nunca entram em `rules`.
    pub states: HashMap<String, HashMap<PseudoState, StyleRule>>,
    /// Regras de seletor de **id** (`#nome { }`), por id (sem o `#`). Mesma
    /// mecânica de `rules`, mas casadas pelo atributo `id` do nó em vez da
    /// `class`, e aplicadas com **especificidade maior** (por cima das classes,
    /// por baixo dos atributos inline) — ver [`resolve_classes`].
    pub ids: HashMap<String, StyleRule>,
    /// `#nome:estado { }` — pseudo-estados de seletor de id (espelho de
    /// [`StyleSheet::states`] para ids).
    pub id_states: HashMap<String, HashMap<PseudoState, StyleRule>>,
    /// Regras de seletor de **tag** (`Button { }`, `Card { }`), por nome de tag
    /// **em minúsculo** (`Button {}` == `button {}`). Casadas por dois caminhos,
    /// ambos com a **menor especificidade** (abaixo de classe/id/inline): o tipo
    /// builtin do nó (`Column`, `Text`, …) e o **nome de um componente** no seu
    /// uso (`<Card/>`) — neste caso aplicadas como underlay na raiz avaliada do
    /// template, já que o componente é inlinado. Ver [`resolve_classes`].
    pub tags: HashMap<String, StyleRule>,
    /// `Tag:estado { }` — pseudo-estados de seletor de tag (espelho de
    /// [`StyleSheet::states`] para tags).
    pub tag_states: HashMap<String, HashMap<PseudoState, StyleRule>>,
}

impl StyleSheet {
    /// `true` se este sheet (ou algum de seus blocos `@media`) declara qualquer
    /// seletor de **tag**. Usado por `eval.rs` para manter o *fast-path* dos nós
    /// sem `class`/`id`: só se houver regra de tag em jogo é que vale a pena
    /// resolver estilo para um nó "pelado".
    pub fn has_tag_rules(&self) -> bool {
        !self.tags.is_empty()
            || !self.tag_states.is_empty()
            || self.media.iter().any(|m| !m.tags.is_empty() || !m.tag_states.is_empty())
    }
}

impl StyleSheet {
    /// Parses an `.gss` source string into a [`StyleSheet`].
    pub fn parse(input: &str) -> Result<Self, String> {
        parse_gss(input)
    }
}

/// Merges the node's optional `tag`, its named classes (a whitespace-separated
/// `class="a b c"` string) and its optional `id` across the given stylesheets
/// into a single [`StyleRule`], by ascending specificity: **tag < class < id**
/// (with the caller's inline attributes winning over all three, applied in
/// `eval.rs`).
///
/// Classes are applied left-to-right (later classes override earlier ones).
/// For a given selector, later stylesheets in the slice take priority, so
/// callers can layer files by ascending priority (e.g. global sheets first,
/// then a component's own scoped sheets). The `tag` is the node's builtin kind
/// (`column`, `text`, …) — or, when resolving a component's underlay, the
/// component's own name; passing `""`/`None` for the other two yields just the
/// tag tier.
pub fn resolve_classes(
    tag: Option<&str>,
    classes: &str,
    id: Option<&str>,
    sheets: &[&StyleSheet],
    viewport: Option<(f32, f32)>,
) -> StyleRule {
    let mut merged = StyleRule::default();
    // Ordem por **especificidade** (baixa → alta), e dentro de cada tier a
    // regra base primeiro e o overlay de `@media` por cima: assim classe vence
    // tag, id vence classe, e o inline (aplicado no eval, por cima de tudo)
    // vence o id — como no CSS. Um `#id` fora de `@media` ainda vence uma
    // `.classe` dentro de `@media` (tier mais alto ganha).

    // Tier 0 — tag (base, depois `@media`). Menor especificidade.
    if let Some(tag) = tag {
        for sheet in sheets {
            if let Some(rule) = sheet.tags.get(tag) {
                merged.merge_from(rule);
            }
        }
        if let Some((w, h)) = viewport {
            for sheet in sheets {
                for mq in &sheet.media {
                    if mq.condition.matches(w, h) {
                        if let Some(rule) = mq.tags.get(tag) {
                            merged.merge_from(rule);
                        }
                    }
                }
            }
        }
    }

    // Tier 1 — classes (base, depois `@media`).
    for name in classes.split_whitespace() {
        for sheet in sheets {
            if let Some(rule) = sheet.rules.get(name) {
                merged.merge_from(rule);
            }
        }
    }
    if let Some((w, h)) = viewport {
        for name in classes.split_whitespace() {
            for sheet in sheets {
                for mq in &sheet.media {
                    if mq.condition.matches(w, h) {
                        if let Some(rule) = mq.rules.get(name) {
                            merged.merge_from(rule);
                        }
                    }
                }
            }
        }
    }

    // Tier 2 — id (base, depois `@media`), POR CIMA das classes.
    if let Some(id) = id {
        for sheet in sheets {
            if let Some(rule) = sheet.ids.get(id) {
                merged.merge_from(rule);
            }
        }
        if let Some((w, h)) = viewport {
            for sheet in sheets {
                for mq in &sheet.media {
                    if mq.condition.matches(w, h) {
                        if let Some(rule) = mq.ids.get(id) {
                            merged.merge_from(rule);
                        }
                    }
                }
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

/// Mesma lógica de [`resolve_classes`] (classes → id → sheets → `@media` →
/// `var()`), mas para os blocos `.classe:estado { }` e `#id:estado { }`:
/// devolve os 4 overlays possíveis, cada um vazio (`StyleRule::default()`)
/// quando nem classe nem id da lista declara aquele estado em nenhum sheet
/// ativo. O `#id:estado` sobrepõe o `.classe:estado`.
pub fn resolve_state_classes(
    tag: Option<&str>,
    classes: &str,
    id: Option<&str>,
    sheets: &[&StyleSheet],
    viewport: Option<(f32, f32)>,
) -> StateStyles {
    let mut out = StateStyles::default();
    // Tier 0 — tag (base, depois `@media`). Menor especificidade.
    if let Some(tag) = tag {
        for sheet in sheets {
            if let Some(by_state) = sheet.tag_states.get(tag) {
                for (state, rule) in by_state {
                    out.get_mut(*state).merge_from(rule);
                }
            }
        }
        if let Some((w, h)) = viewport {
            for sheet in sheets {
                for mq in &sheet.media {
                    if mq.condition.matches(w, h) {
                        if let Some(by_state) = mq.tag_states.get(tag) {
                            for (state, rule) in by_state {
                                out.get_mut(*state).merge_from(rule);
                            }
                        }
                    }
                }
            }
        }
    }
    // Tier 1 — classes (base, depois `@media`).
    for name in classes.split_whitespace() {
        for sheet in sheets {
            if let Some(by_state) = sheet.states.get(name) {
                for (state, rule) in by_state {
                    out.get_mut(*state).merge_from(rule);
                }
            }
        }
    }
    if let Some((w, h)) = viewport {
        for name in classes.split_whitespace() {
            for sheet in sheets {
                for mq in &sheet.media {
                    if mq.condition.matches(w, h) {
                        if let Some(by_state) = mq.states.get(name) {
                            for (state, rule) in by_state {
                                out.get_mut(*state).merge_from(rule);
                            }
                        }
                    }
                }
            }
        }
    }
    // Tier 2 — id (base, depois `@media`), POR CIMA das classes.
    if let Some(id) = id {
        for sheet in sheets {
            if let Some(by_state) = sheet.id_states.get(id) {
                for (state, rule) in by_state {
                    out.get_mut(*state).merge_from(rule);
                }
            }
        }
        if let Some((w, h)) = viewport {
            for sheet in sheets {
                for mq in &sheet.media {
                    if mq.condition.matches(w, h) {
                        if let Some(by_state) = mq.id_states.get(id) {
                            for (state, rule) in by_state {
                                out.get_mut(*state).merge_from(rule);
                            }
                        }
                    }
                }
            }
        }
    }
    let mut vars: HashMap<String, String> = HashMap::new();
    for sheet in sheets {
        for (k, v) in &sheet.variables {
            vars.insert(k.clone(), v.clone());
        }
    }
    out.hover.resolve_var_refs(&vars);
    out.focus.resolve_var_refs(&vars);
    out.active.resolve_var_refs(&vars);
    out.disabled.resolve_var_refs(&vars);
    out
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
    let mut media: Vec<MediaQuery> = Vec::new();
    let mut states: HashMap<String, HashMap<PseudoState, StyleRule>> = HashMap::new();
    let mut ids: HashMap<String, StyleRule> = HashMap::new();
    let mut id_states: HashMap<String, HashMap<PseudoState, StyleRule>> = HashMap::new();
    let mut tags: HashMap<String, StyleRule> = HashMap::new();
    let mut tag_states: HashMap<String, HashMap<PseudoState, StyleRule>> = HashMap::new();
    let mut rest = cleaned.as_str();
    while let Some(open) = rest.find('{') {
        let selector = rest[..open].trim();
        let after_open = &rest[open + 1..];

        // `@media (cond) { .a {…} .b {…} }` — bloco com regras ANINHADAS, então
        // a chave de fechamento precisa ser casada por profundidade (não o
        // primeiro `}`, que fecharia a 1ª regra interna). O corpo é reparseado
        // como um mini-sheet (`rules` e `states` interessam; `variables`
        // aninhadas num `@media` não são suportadas — `:root` fica de fora).
        if selector.starts_with("@media") {
            let (inner, remainder) = split_balanced_block(after_open)?;
            let condition = parse_media_condition(selector)?;
            let inner_sheet = parse_gss(inner)?;
            media.push(MediaQuery {
                condition,
                rules: inner_sheet.rules,
                states: inner_sheet.states,
                ids: inner_sheet.ids,
                id_states: inner_sheet.id_states,
                tags: inner_sheet.tags,
                tag_states: inner_sheet.tag_states,
            });
            rest = remainder;
            continue;
        }

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

        // `#nome { }` / `#nome:estado { }` — seletor de id. Guardado à parte em
        // `ids`/`id_states` (nunca em `rules`/`states`), casado depois pelo
        // atributo `id` do nó. Mesma mecânica de estado das classes.
        if let Some(raw) = selector.strip_prefix('#') {
            let raw = raw.trim();
            if raw.is_empty() {
                return Err("Empty id selector '#'".to_string());
            }
            if let Some((name, state_str)) = raw.split_once(':') {
                let name = name.trim();
                if name.is_empty() {
                    return Err(format!("Empty id name in pseudo-state selector '#{}'", raw));
                }
                let state = PseudoState::parse(state_str.trim()).ok_or_else(|| {
                    format!("Unsupported pseudo-state ':{}' in selector '#{}'", state_str.trim(), raw)
                })?;
                let rule = parse_rule_body(body, raw)?;
                id_states
                    .entry(name.to_string())
                    .or_default()
                    .entry(state)
                    .or_default()
                    .merge_from(&rule);
                continue;
            }
            let rule = parse_rule_body(body, raw)?;
            ids.entry(raw.to_string()).or_default().merge_from(&rule);
            continue;
        }

        // Seletor de **tag** — qualquer identificador que não seja
        // `.classe`/`#id`/`:root`/`@media` (esses já foram tratados acima).
        // Casa o tipo builtin do nó (`Column`, `Text`, …) OU o nome de um
        // componente (`Card`) no seu uso. Normalizado para minúsculo, então
        // `Button {}` == `button {}`. Aceita `Tag:estado`.
        if !selector.starts_with('.') {
            let raw = selector.trim();
            if raw.is_empty() {
                return Err("Empty selector before '{'".to_string());
            }
            if let Some((name, state_str)) = raw.split_once(':') {
                let name = name.trim().to_lowercase();
                if name.is_empty() {
                    return Err(format!("Empty tag name in pseudo-state selector '{}'", raw));
                }
                let state = PseudoState::parse(state_str.trim()).ok_or_else(|| {
                    format!("Unsupported pseudo-state ':{}' in selector '{}'", state_str.trim(), raw)
                })?;
                let rule = parse_rule_body(body, raw)?;
                tag_states
                    .entry(name)
                    .or_default()
                    .entry(state)
                    .or_default()
                    .merge_from(&rule);
                continue;
            }
            let rule = parse_rule_body(body, raw)?;
            tags.entry(raw.to_lowercase()).or_default().merge_from(&rule);
            continue;
        }
        let raw = selector[1..].trim();
        if raw.is_empty() {
            return Err("Empty class selector '.'".to_string());
        }

        // `.classe:estado { }` — pseudo-estado (`:hover`/`:focus`/`:active`/
        // `:disabled`), guardado à parte em `states` (nunca em `rules`).
        if let Some((name, state_str)) = raw.split_once(':') {
            let name = name.trim();
            if name.is_empty() {
                return Err(format!("Empty class name in pseudo-state selector '.{}'", raw));
            }
            let state = PseudoState::parse(state_str.trim()).ok_or_else(|| {
                format!("Unsupported pseudo-state ':{}' in selector '.{}'", state_str.trim(), raw)
            })?;
            let rule = parse_rule_body(body, raw)?;
            states
                .entry(name.to_string())
                .or_default()
                .entry(state)
                .or_default()
                .merge_from(&rule);
            continue;
        }

        let name = raw.to_string();
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

    Ok(StyleSheet { rules, variables, media, states, ids, id_states, tags, tag_states })
}

/// Dado o texto logo APÓS o `{` de um bloco, devolve `(interior, resto)` onde
/// `interior` vai até a `}` que casa (por profundidade de chaves) e `resto` é o
/// que vem depois dela. Erra se o bloco não fecha.
fn split_balanced_block(s: &str) -> Result<(&str, &str), String> {
    let mut depth = 1usize;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((&s[..i], &s[i + 1..]));
                }
            }
            _ => {}
        }
    }
    Err("Unclosed `@media { ... }` block".to_string())
}

/// Parseia a condição de um `@media` a partir do seletor bruto, ex.:
/// `@media (max-width: 800)` ou `@media (min-width: 600) and (max-width: 900)`.
/// Coleta todas as features `(chave: número)` com semântica AND; palavras como
/// `and`/`screen`/`all` fora dos parênteses são ignoradas.
fn parse_media_condition(selector: &str) -> Result<MediaCondition, String> {
    let mut cond = MediaCondition::default();
    let mut s = selector.strip_prefix("@media").unwrap_or(selector);
    while let Some(open) = s.find('(') {
        let close = s[open..]
            .find(')')
            .ok_or_else(|| format!("Missing ')' in @media condition '{}'", selector))?
            + open;
        let feature = &s[open + 1..close];
        let (key, value) = feature
            .split_once(':')
            .ok_or_else(|| format!("Invalid @media feature '{}'", feature))?;
        let key = key.trim();
        let val = value
            .trim()
            .trim_end_matches("px")
            .trim()
            .parse::<f32>()
            .map_err(|_| format!("@media feature '{}' expects a number", key))?;
        match key {
            "min-width" => cond.min_width = Some(val),
            "max-width" => cond.max_width = Some(val),
            "min-height" => cond.min_height = Some(val),
            "max-height" => cond.max_height = Some(val),
            other => return Err(format!("Unsupported @media feature '{}'", other)),
        }
        s = &s[close + 1..];
    }
    Ok(cond)
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
            // `hidden: true|false` ou `display: none|flex|block` (só `none`
            // esconde; qualquer outro valor de `display` = visível).
            "hidden" => rule.hidden = Some(value.eq_ignore_ascii_case("true") || value == "1"),
            "display" => rule.hidden = Some(value.eq_ignore_ascii_case("none")),
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
        let merged = resolve_classes(None, "a b", None, &[&base, &over], None);
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
    fn bare_identifier_is_a_tag_selector() {
        // Antes um erro ("must start with '.'"); agora um seletor de tag, guardado
        // em `tags` (minúsculo), não em `rules`.
        let sheet = parse_gss("Card { padding: 1; }").unwrap();
        assert!(sheet.rules.is_empty());
        assert_eq!(sheet.tags.get("card").unwrap().padding.as_deref(), Some("1"));
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
        let r = resolve_classes(None, "card", None, &[&sheet], None);
        assert_eq!(r.background.as_deref(), Some("#0D1117"));
        assert_eq!(r.color.as_deref(), Some("#58A6FF"));
    }

    #[test]
    fn var_fallback_and_undefined() {
        let sheet = parse_gss(".x { color: var(--missing, #FF0000); background: var(--nope); }").unwrap();
        let r = resolve_classes(None, "x", None, &[&sheet], None);
        assert_eq!(r.color.as_deref(), Some("#FF0000")); // usa o fallback
        assert_eq!(r.background.as_deref(), Some("")); // sem var nem fallback → vazio
    }

    #[test]
    fn vars_are_cross_sheet() {
        // Paleta declarada num sheet (global), usada por regra de outro (escopo).
        let global = parse_gss(":root { --ok: #3FB950; }").unwrap();
        let scoped = parse_gss(".state { color: var(--ok); }").unwrap();
        let r = resolve_classes(None, "state", None, &[&global, &scoped], None);
        assert_eq!(r.color.as_deref(), Some("#3FB950"));
    }

    #[test]
    fn var_embedded_in_gradient() {
        let sheet = parse_gss(
            ":root { --a: #000000; --b: #FFFFFF; } .g { gradient: var(--a) var(--b); }",
        )
        .unwrap();
        let r = resolve_classes(None, "g", None, &[&sheet], None);
        assert_eq!(r.gradient.as_deref(), Some("#000000 #FFFFFF"));
    }

    #[test]
    fn media_max_width_overrides_when_narrow() {
        let sheet = parse_gss(
            ".panel { width: 640; } @media (max-width: 800) { .panel { width: fill; } }",
        )
        .unwrap();
        assert_eq!(sheet.media.len(), 1);
        // Largo (1000 > 800): media inativa → base.
        let wide = resolve_classes(None, "panel", None, &[&sheet], Some((1000.0, 700.0)));
        assert_eq!(wide.width.as_deref(), Some("640"));
        // Estreito (700 <= 800): media ativa → sobrescreve.
        let narrow = resolve_classes(None, "panel", None, &[&sheet], Some((700.0, 700.0)));
        assert_eq!(narrow.width.as_deref(), Some("fill"));
        // Sem viewport: media nunca ativa.
        let none = resolve_classes(None, "panel", None, &[&sheet], None);
        assert_eq!(none.width.as_deref(), Some("640"));
    }

    #[test]
    fn hidden_property_and_display_none() {
        let sheet = parse_gss(
            ".a { hidden: true; } .b { display: none; } .c { display: flex; } .d { hidden: 0; }",
        )
        .unwrap();
        assert_eq!(sheet.rules.get("a").unwrap().hidden, Some(true));
        assert_eq!(sheet.rules.get("b").unwrap().hidden, Some(true));
        assert_eq!(sheet.rules.get("c").unwrap().hidden, Some(false));
        assert_eq!(sheet.rules.get("d").unwrap().hidden, Some(false));
    }

    #[test]
    fn media_can_hide_at_narrow_width() {
        // O caso de uso central: `@media` esconde um elemento em telas estreitas.
        let sheet = parse_gss(
            "@media (max-width: 600) { .search { hidden: true; } }",
        )
        .unwrap();
        let wide = resolve_classes(None, "search", None, &[&sheet], Some((1000.0, 700.0)));
        assert_eq!(wide.hidden, None); // visível (nada aplicado)
        let narrow = resolve_classes(None, "search", None, &[&sheet], Some((500.0, 700.0)));
        assert_eq!(narrow.hidden, Some(true)); // escondido
    }

    #[test]
    fn media_condition_range_and_height() {
        let c = MediaCondition {
            min_width: Some(600.0),
            max_width: Some(900.0),
            max_height: Some(500.0),
            ..Default::default()
        };
        assert!(c.matches(800.0, 400.0)); // dentro de tudo
        assert!(!c.matches(500.0, 400.0)); // largura < min
        assert!(!c.matches(1000.0, 400.0)); // largura > max
        assert!(!c.matches(800.0, 600.0)); // altura > max
    }

    #[test]
    fn parses_pseudo_state_selectors() {
        let sheet = parse_gss(
            ".btn { background: #111111; } \
             .btn:hover { background: #222222; } \
             .btn:disabled { background: #333333; text-color: #999999; }",
        )
        .unwrap();
        assert_eq!(sheet.rules["btn"].background.as_deref(), Some("#111111"));
        let states = resolve_state_classes(None, "btn", None, &[&sheet], None);
        assert_eq!(states.hover.background.as_deref(), Some("#222222"));
        assert_eq!(states.disabled.background.as_deref(), Some("#333333"));
        assert_eq!(states.disabled.text_color.as_deref(), Some("#999999"));
        assert_eq!(states.focus.background, None); // não declarado
        assert_eq!(states.active.background, None);
    }

    #[test]
    fn unsupported_pseudo_state_is_an_error() {
        assert!(parse_gss(".btn:wobble { color: #111111; }").is_err());
    }

    #[test]
    fn duplicate_pseudo_state_merges_not_clobbers() {
        let sheet = parse_gss(
            ".btn:hover { background: #222222; } .btn:hover { text-color: #ffffff; }",
        )
        .unwrap();
        let states = resolve_state_classes(None, "btn", None, &[&sheet], None);
        assert_eq!(states.hover.background.as_deref(), Some("#222222"));
        assert_eq!(states.hover.text_color.as_deref(), Some("#ffffff"));
    }

    #[test]
    fn pseudo_state_vars_resolve() {
        let sheet = parse_gss(
            ":root { --hoverbg: #abcdef; } .btn:hover { background: var(--hoverbg); }",
        )
        .unwrap();
        let states = resolve_state_classes(None, "btn", None, &[&sheet], None);
        assert_eq!(states.hover.background.as_deref(), Some("#abcdef"));
    }

    #[test]
    fn pseudo_state_inside_media_applies_when_matched() {
        let sheet = parse_gss(
            "@media (max-width: 500) { .btn:hover { background: #000000; } }",
        )
        .unwrap();
        let narrow = resolve_state_classes(None, "btn", None, &[&sheet], Some((400.0, 400.0)));
        assert_eq!(narrow.hover.background.as_deref(), Some("#000000"));
        let wide = resolve_state_classes(None, "btn", None, &[&sheet], Some((900.0, 400.0)));
        assert_eq!(wide.hover.background, None);
        let no_viewport = resolve_state_classes(None, "btn", None, &[&sheet], None);
        assert_eq!(no_viewport.hover.background, None);
    }

    #[test]
    fn media_nested_braces_parse_correctly() {
        // A `}` da 1ª regra interna não pode fechar o @media (casamento por
        // profundidade). Duas classes dentro, mais uma fora.
        let sheet = parse_gss(
            ".a { color: #111; } \
             @media (max-width: 500) { .a { color: #222; } .b { size: 9; } } \
             .c { color: #333; }",
        )
        .unwrap();
        assert_eq!(sheet.rules.len(), 2); // .a e .c fora do media
        assert_eq!(sheet.media[0].rules.len(), 2); // .a e .b dentro
        let narrow = resolve_classes(None, "a", None, &[&sheet], Some((400.0, 400.0)));
        assert_eq!(narrow.color.as_deref(), Some("#222"));
        let wide = resolve_classes(None, "a", None, &[&sheet], Some((900.0, 400.0)));
        assert_eq!(wide.color.as_deref(), Some("#111"));
    }

    #[test]
    fn parses_id_selector() {
        let sheet = parse_gss("#save { background: #2E3440; padding: 8; }").unwrap();
        assert!(sheet.rules.is_empty()); // id não entra em `rules`
        let save = sheet.ids.get("save").unwrap();
        assert_eq!(save.background.as_deref(), Some("#2E3440"));
        assert_eq!(save.padding.as_deref(), Some("8"));
    }

    #[test]
    fn empty_id_selector_is_an_error() {
        assert!(parse_gss("# { padding: 1; }").is_err());
    }

    #[test]
    fn id_resolves_when_matched_and_not_otherwise() {
        let sheet = parse_gss("#save { color: #111111; }").unwrap();
        let hit = resolve_classes(None, "", Some("save"), &[&sheet], None);
        assert_eq!(hit.color.as_deref(), Some("#111111"));
        // id diferente não casa; sem id também não.
        assert_eq!(resolve_classes(None, "", Some("other"), &[&sheet], None).color, None);
        assert_eq!(resolve_classes(None, "", None, &[&sheet], None).color, None);
    }

    #[test]
    fn id_wins_over_class_specificity() {
        // id é tier mais alto: sobrepõe a classe, inclusive uma classe dentro de
        // `@media` (id fora de media ainda vence).
        let sheet = parse_gss(
            ".panel { color: #aaaaaa; background: #000000; } \
             #hero { color: #ffffff; } \
             @media (max-width: 800) { .panel { color: #cccccc; } }",
        )
        .unwrap();
        let r = resolve_classes(None, "panel", Some("hero"), &[&sheet], Some((400.0, 400.0)));
        assert_eq!(r.color.as_deref(), Some("#ffffff")); // id vence a classe (e a classe em @media)
        assert_eq!(r.background.as_deref(), Some("#000000")); // campo só na classe, preservado
    }

    #[test]
    fn id_duplicate_merges_and_is_cross_sheet() {
        let global = parse_gss(":root { --ok: #3FB950; }").unwrap();
        let a = parse_gss("#x { padding: 4; color: var(--ok); }").unwrap();
        let b = parse_gss("#x { color: #222222; }").unwrap();
        let r = resolve_classes(None, "", Some("x"), &[&global, &a, &b], None);
        assert_eq!(r.padding.as_deref(), Some("4")); // preservado do 1º sheet
        assert_eq!(r.color.as_deref(), Some("#222222")); // sheet posterior vence
    }

    #[test]
    fn id_pseudo_states_resolve() {
        let sheet = parse_gss(
            "#save { background: #111111; } \
             #save:hover { background: #222222; } \
             #save:disabled { background: #333333; }",
        )
        .unwrap();
        assert_eq!(sheet.ids["save"].background.as_deref(), Some("#111111"));
        let states = resolve_state_classes(None, "", Some("save"), &[&sheet], None);
        assert_eq!(states.hover.background.as_deref(), Some("#222222"));
        assert_eq!(states.disabled.background.as_deref(), Some("#333333"));
        assert_eq!(states.focus.background, None);
    }

    #[test]
    fn id_state_overrides_class_state() {
        // `#id:hover` (tier alto) vence `.classe:hover` (tier baixo).
        let sheet = parse_gss(
            ".btn:hover { background: #aaaaaa; } #go:hover { background: #ffffff; }",
        )
        .unwrap();
        let states = resolve_state_classes(None, "btn", Some("go"), &[&sheet], None);
        assert_eq!(states.hover.background.as_deref(), Some("#ffffff"));
    }

    #[test]
    fn id_inside_media_overrides_when_matched() {
        let sheet = parse_gss(
            "#panel { width: 640; } @media (max-width: 800) { #panel { width: fill; } }",
        )
        .unwrap();
        assert_eq!(sheet.media[0].ids.len(), 1);
        let wide = resolve_classes(None, "", Some("panel"), &[&sheet], Some((1000.0, 700.0)));
        assert_eq!(wide.width.as_deref(), Some("640"));
        let narrow = resolve_classes(None, "", Some("panel"), &[&sheet], Some((700.0, 700.0)));
        assert_eq!(narrow.width.as_deref(), Some("fill"));
    }

    #[test]
    fn parses_tag_selector_normalized_lowercase() {
        let sheet = parse_gss("Button { padding: 8; } column { spacing: 12; }").unwrap();
        assert!(sheet.rules.is_empty() && sheet.ids.is_empty());
        assert_eq!(sheet.tags.get("button").unwrap().padding.as_deref(), Some("8"));
        assert_eq!(sheet.tags.get("column").unwrap().spacing, Some(12.0));
        assert!(sheet.has_tag_rules());
    }

    #[test]
    fn tag_is_lowest_specificity() {
        // tag < classe < id: cada tier sobrepõe o anterior.
        let sheet = parse_gss(
            "Button { color: #aaaaaa; padding: 4; } \
             .btn { color: #bbbbbb; } \
             #go { color: #cccccc; }",
        )
        .unwrap();
        // só tag: pega a cor da tag e o padding.
        let only_tag = resolve_classes(Some("button"), "", None, &[&sheet], None);
        assert_eq!(only_tag.color.as_deref(), Some("#aaaaaa"));
        assert_eq!(only_tag.padding.as_deref(), Some("4"));
        // tag + classe: classe vence a cor; padding da tag preservado.
        let tag_class = resolve_classes(Some("button"), "btn", None, &[&sheet], None);
        assert_eq!(tag_class.color.as_deref(), Some("#bbbbbb"));
        assert_eq!(tag_class.padding.as_deref(), Some("4"));
        // tag + classe + id: id vence tudo.
        let all = resolve_classes(Some("button"), "btn", Some("go"), &[&sheet], None);
        assert_eq!(all.color.as_deref(), Some("#cccccc"));
        assert_eq!(all.padding.as_deref(), Some("4"));
    }

    #[test]
    fn tag_pseudo_states_and_media() {
        let sheet = parse_gss(
            "Button:hover { background: #222222; } \
             @media (max-width: 500) { Button { background: #333333; } }",
        )
        .unwrap();
        let states = resolve_state_classes(Some("button"), "", None, &[&sheet], None);
        assert_eq!(states.hover.background.as_deref(), Some("#222222"));
        // Base da tag só dentro do @media, quando estreito.
        let narrow = resolve_classes(Some("button"), "", None, &[&sheet], Some((400.0, 400.0)));
        assert_eq!(narrow.background.as_deref(), Some("#333333"));
        let wide = resolve_classes(Some("button"), "", None, &[&sheet], Some((900.0, 400.0)));
        assert_eq!(wide.background, None);
    }

    #[test]
    fn tag_and_component_share_the_tags_map() {
        // Builtin (`Column`) e componente (`Card`) vivem no mesmo mapa `tags`,
        // ambos casados por nome minúsculo — o underlay de componente é só
        // `resolve_classes(Some("card"), "", None, ...)`.
        let sheet = parse_gss("Card { padding: 24; } Column { spacing: 8; }").unwrap();
        assert_eq!(resolve_classes(Some("card"), "", None, &[&sheet], None).padding.as_deref(), Some("24"));
        assert_eq!(resolve_classes(Some("column"), "", None, &[&sheet], None).spacing, Some(8.0));
    }

    #[test]
    fn has_tag_rules_false_without_tag_selectors() {
        let sheet = parse_gss(".a { color: #111; } #b { color: #222; }").unwrap();
        assert!(!sheet.has_tag_rules());
    }
}
