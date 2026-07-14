use std::collections::HashMap;
use crate::error::Result;
use crate::parser::{UiNode, NodeType, NumAttr};
use crate::stylesheet::{StyleSheet, StyleRule, StateStyles, resolve_classes, resolve_state_classes};

/// Splits a `<script>...</script>` block out of an XML document, returning the
/// markup with the block removed and the script body (if any).
///
/// The script is stripped *before* XML parsing, so it may sit as a sibling of
/// the root element (it would otherwise make the document multi-rooted). The
/// markup parser ignores the script; its Lua body is interpreted at runtime by
/// [`crate::luau::LuauComponent`].
///
/// O bloco é substituído por **tantas quebras de linha quantas ele ocupava**, em
/// vez de simplesmente sumir. Sem isso, todo o markup abaixo de um `<script>`
/// inline de 30 linhas subiria 30 linhas aos olhos do parser de XML — e um erro
/// na linha 80 sairia reportado como linha 50, que é pior do que não ter linha
/// nenhuma: manda o autor olhar para um trecho inocente.
pub fn strip_script(xml: &str) -> (String, Option<String>) {
    let Some(open_start) = find_script_open(xml) else {
        return (xml.to_string(), None);
    };
    // Find the end of the opening tag (supports `<script>` and `<script ...>`).
    let Some(gt_rel) = xml[open_start..].find('>') else {
        return (xml.to_string(), None);
    };
    let body_start = open_start + gt_rel + 1;
    let lower_tail = xml[body_start..].to_ascii_lowercase();
    let Some(close_rel) = lower_tail.find("</script>") else {
        return (xml.to_string(), None);
    };

    let body_end = body_start + close_rel;
    let close_end = body_end + "</script>".len();
    let script = xml[body_start..body_end].to_string();

    let mut markup = String::with_capacity(xml.len());
    markup.push_str(&xml[..open_start]);
    for _ in 0..xml[open_start..close_end].matches('\n').count() {
        markup.push('\n');
    }
    markup.push_str(&xml[close_end..]);
    (markup, Some(script))
}

/// Índice do `<script` que abre o bloco de script — ignorando um citado dentro
/// de um comentário XML (`<!-- <script> -->`), que não é um bloco de verdade.
fn find_script_open(xml: &str) -> Option<usize> {
    let lower = xml.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find("<script").map(|i| from + i) {
        // Dentro de um comentário? Basta olhar para trás: se o `<!--` mais
        // recente ainda não foi fechado por um `-->`, estamos comentados.
        let before = &lower[..i];
        let open = before.rfind("<!--");
        let closed = open.is_none_or(|o| before[o..].contains("-->"));
        if closed {
            return Some(i);
        }
        from = i + 7;
    }
    None
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

/// O contexto **durante a avaliação**: a base (o contexto do motor) mais uma
/// cadeia de camadas com as variáveis locais — as vars de um item de `for-each`
/// (`{item.nome}`) e as props de um componente.
///
/// Existe para não **clonar a base**. A versão anterior fazia
/// `let mut local_context = context.clone()` por **item** de lista: com 45 linhas
/// na tela e um log de 100 KB no contexto, isso é copiar ~5 MB de string por
/// reavaliação — e a reavaliação roda a cada tecla e a cada mensagem do SSE. Era
/// o que fazia uma árvore de 600 nós custar 6,5 ms quando os nós em si custam
/// uma fração disso.
///
/// A busca vai da camada mais **interna** para a mais externa e só então na base,
/// então uma var local sombreia uma chave global de mesmo nome — exatamente o que
/// o `insert` sobre o clone fazia. As camadas têm poucas entradas (os campos de
/// um item), então a varredura linear é mais barata que um `HashMap`.
#[derive(Clone, Copy)]
pub struct EvalCtx<'a> {
    base: &'a HashMap<String, String>,
    /// A camada mais interna; cada uma aponta para a de fora (lista ligada na
    /// pilha, sem alocação).
    layer: Option<&'a Layer<'a>>,
}

/// Um conjunto de variáveis locais empilhado sobre o contexto. Ver [`EvalCtx`].
pub struct Layer<'a> {
    vars: Vec<(String, String)>,
    outer: Option<&'a Layer<'a>>,
}

impl<'a> Layer<'a> {
    fn new(outer: Option<&'a Layer<'a>>) -> Self {
        Self { vars: Vec::new(), outer }
    }

    fn set(&mut self, key: String, value: String) {
        // Uma chave repetida na MESMA camada sobrescreve (semântica de `insert`).
        match self.vars.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.vars.push((key, value)),
        }
    }

    fn get(&self, key: &str) -> Option<&str> {
        let mut cur = Some(self);
        while let Some(l) = cur {
            if let Some((_, v)) = l.vars.iter().find(|(k, _)| k == key) {
                return Some(v);
            }
            cur = l.outer;
        }
        None
    }
}

impl<'a> EvalCtx<'a> {
    /// Contexto de avaliação sobre `base`, sem nenhuma camada local.
    pub fn new(base: &'a HashMap<String, String>) -> Self {
        Self { base, layer: None }
    }

    /// O valor de `key`: camadas locais (da mais interna para a mais externa)
    /// primeiro, base depois.
    pub fn get(&self, key: &str) -> Option<&str> {
        match self.layer.and_then(|l| l.get(key)) {
            Some(v) => Some(v),
            None => self.base.get(key).map(String::as_str),
        }
    }

    /// O mesmo contexto com `layer` empilhada por cima (a camada precisa viver
    /// no frame do chamador — é isso que torna a operação O(1), sem cópia).
    fn with<'c>(&self, layer: &'c Layer<'c>) -> EvalCtx<'c>
    where
        'a: 'c,
    {
        EvalCtx { base: self.base, layer: Some(layer) }
    }

    /// A camada corrente, para uma nova ser encadeada sob ela.
    fn layer(&self) -> Option<&'a Layer<'a>> {
        self.layer
    }
}

/// Monta a camada de variáveis de **um item** de `for-each`: `{var.campo}` para
/// cada campo de um objeto, ou `{var}` para um escalar. Devolve também a
/// identidade do item (o valor de `reorder_key`), de que o drag-and-drop precisa.
///
/// Substitui o antigo `context.clone()` + `insert` por item — ver [`EvalCtx`].
fn item_layer<'b>(
    item: &serde_json::Value,
    var: &str,
    reorder_key: Option<&str>,
    context: &EvalCtx<'b>,
) -> (Layer<'b>, Option<String>) {
    let mut layer = Layer::new(context.layer());
    let mut this_key: Option<String> = None;

    match item {
        serde_json::Value::Object(obj) => {
            for (key, val) in obj {
                let str_val = match val {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                if reorder_key == Some(key.as_str()) {
                    this_key = Some(str_val.clone());
                }
                layer.set(format!("{var}.{key}"), str_val);
            }
        }
        serde_json::Value::String(s) => layer.set(var.to_string(), s.clone()),
        other => layer.set(var.to_string(), other.to_string()),
    }

    // Drag highlight: expõe se ESTE item é o que está sendo arrastado, para o
    // template poder estilizar a linha agarrada (ver `crate::DRAG_KEY_CONTEXT`).
    if let Some(key) = &this_key {
        let dragging = context.get(crate::DRAG_KEY_CONTEXT) == Some(key.as_str());
        layer.set(format!("{var}.__dragging"), dragging.to_string());
    }

    (layer, this_key)
}

/// Process string template by replacing `{key}` placeholders with values from context
pub fn process_template(template: &str, context: &HashMap<String, String>) -> String {
    process_tpl(template, &EvalCtx::new(context))
}

/// O `process_template` de verdade, sobre o [`EvalCtx`] (o público acima é a
/// casca para quem só tem um `HashMap` em mãos).
fn process_tpl(template: &str, context: &EvalCtx) -> String {
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
                // Inline default: `{key|default}` uses `default` (the text after
                // the first `|`) when `key` is absent from the context. Without a
                // `|` the behavior is unchanged: a missing key resolves to empty.
                // This is what lets a component default its own props per instance
                // without seeding — or polluting — the global context.
                let (lookup, default) = match key.split_once('|') {
                    Some((k, d)) => (k.trim(), Some(d.trim())),
                    None => (key.trim(), None),
                };
                if let Some(val) = context.get(lookup) {
                    result.push_str(val);
                } else if let Some(d) = default {
                    result.push_str(d);
                }
                // else: unknown key with no default -> empty (unchanged).
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
    context: &EvalCtx,
) -> bool {
    let value = process_tpl(cond, context);
    if let Some(eq) = equals {
        return value == process_tpl(eq, context);
    }
    if let Some(ne) = not_equals {
        return value != process_tpl(ne, context);
    }
    is_truthy(&value)
}

/// The stylesheets in effect during evaluation, split by scope.
///
/// `global` sheets apply everywhere: loaded via `GlacierUI::load_stylesheet`,
/// via a `<link rel="stylesheet">`, or an inline `<style>` block without
/// `scoped="true"` — all three land in the same set. `by_component` holds only
/// the sheets a component declared with `<style scoped="true">`, keyed by
/// component name; they apply only inside that component's subtree, layered
/// *on top of* the global ones so a scoped class can override a global one
/// locally.
pub struct StyleContext<'a> {
    pub global: &'a [StyleSheet],
    pub by_component: &'a HashMap<String, Vec<StyleSheet>>,
    /// Tamanho atual do viewport `(largura, altura)` em px lógicos, para avaliar
    /// blocos `@media`. `None` = sem info (nenhuma media query ativa).
    pub viewport: Option<(f32, f32)>,
    /// `true` se qualquer sheet ativo (global ou de escopo) declara seletor de
    /// **tag** — atalho para pular a resolução de estilo em nós sem `class`/`id`
    /// quando não há nenhuma regra de tag para casar (ver [`eval_owned`]).
    pub has_tag_rules: bool,
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
    context: &EvalCtx,
    templates: &HashMap<String, UiNode>,
    styles: &StyleContext,
    scope: Option<&str>,
    owner: Option<&str>,
    out: &mut Vec<UiNode>,
) -> Result<()> {
    // Tracks the result of the immediately preceding `<if>`, so an `<else>`
    // can bind to it. Reset by any other (non-else) node.
    let mut last_if: Option<bool> = None;
    for child in children {
        if matches!(child.kind, NodeType::Import { .. } | NodeType::Link { .. } | NodeType::Style { .. }) {
            continue;
        }

        // 1. Process for-each attribute directive (outer precedence)
        if let Some(items) = &child.for_each {
            let var = child.for_each_var.as_deref().unwrap_or("item");
            let items_evaluated = process_tpl(items, context);
            // Drag-and-drop: resolved once per for-each, reused by every item.
            let reorder_key = child.reorder_key.as_ref().map(|s| process_tpl(s, context));
            let on_reorder = child.on_reorder.as_ref()
                .map(|s| namespace_action(process_tpl(s, context), owner));
            if let Some(json_str) = context.get(&items_evaluated) {
                if let Ok(serde_json::Value::Array(arr)) =
                    serde_json::from_str::<serde_json::Value>(json_str)
                {
                    // Full identity snapshot, needed by the handle's `DragStart`.
                    let full_order: Vec<String> = match &reorder_key {
                        Some(rk) => arr.iter()
                            .filter_map(|item| item.get(rk).and_then(|v| v.as_str()).map(String::from))
                            .collect(),
                        None => Vec::new(),
                    };
                    for item in arr {
                        // Variáveis do item numa CAMADA sobre o contexto, sem
                        // clonar a base (ver `EvalCtx`).
                        let (layer, this_key) =
                            item_layer(&item, var, reorder_key.as_deref(), context);
                        let item_ctx = context.with(&layer);

                        // Clone the child without the for_each directive
                        let mut clone = child.clone();
                        clone.for_each = None;
                        clone.for_each_var = None;
                        clone.on_reorder = None;
                        clone.reorder_key = None;

                        if let (Some(on_reorder), Some(key), Some(rk)) = (&on_reorder, &this_key, &reorder_key) {
                            hydrate_drag_item(
                                std::slice::from_mut(&mut clone),
                                &items_evaluated,
                                key,
                                &full_order,
                                on_reorder,
                                rk,
                            );
                        }

                        // Expand the single child in the new context (which will evaluate its if condition if present)
                        expand_children(
                            std::slice::from_ref(&clone),
                            &item_ctx,
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
                out.push(eval_owned(&clone, context, templates, styles, scope, owner, None, None)?);
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
                out.push(eval_owned(&clone, context, templates, styles, scope, owner, None, None)?);
            }
            last_if = Some(truthy);
            continue;
        }

        // 4. Fallback to legacy tag-based conditionals/loops
        match &child.kind {
            // `<import>`/`<link>`/`<style>` declarations are skipped above.
            NodeType::Import { .. } | NodeType::Link { .. } | NodeType::Style { .. } => {}
            NodeType::ForEach { items, var } => {
                let items_evaluated = process_tpl(items, context);
                // Drag-and-drop: `onReorder`/`reorderKey` on the `<ForEach>` tag
                // itself (a plain node attribute, same as `onPress`/`cursor`).
                let reorder_key = child.reorder_key.as_ref().map(|s| process_tpl(s, context));
                let on_reorder = child.on_reorder.as_ref()
                    .map(|s| namespace_action(process_tpl(s, context), owner));
                if let Some(json_str) = context.get(&items_evaluated) {
                    if let Ok(serde_json::Value::Array(arr)) =
                        serde_json::from_str::<serde_json::Value>(json_str)
                    {
                        let full_order: Vec<String> = match &reorder_key {
                            Some(rk) => arr.iter()
                                .filter_map(|item| item.get(rk).and_then(|v| v.as_str()).map(String::from))
                                .collect(),
                            None => Vec::new(),
                        };
                        for item in arr {
                            // Variáveis do item numa CAMADA sobre o contexto, sem
                            // clonar a base (ver `EvalCtx`).
                            let (layer, this_key) =
                                item_layer(&item, var, reorder_key.as_deref(), context);
                            let item_ctx = context.with(&layer);

                            // The `<ForEach>` tag's body isn't a single node like
                            // the attribute form's — clone its children so the
                            // hydration below has somewhere of its own to live.
                            let mut body: Vec<UiNode> = child.children.clone();
                            if let (Some(on_reorder), Some(key), Some(rk)) = (&on_reorder, &this_key, &reorder_key) {
                                hydrate_drag_item(&mut body, &items_evaluated, key, &full_order, on_reorder, rk);
                            }
                            // Re-run the structural expansion on the body so that
                            // nested `if`/`else`/`ForEach` are honoured per item.
                            expand_children(
                                &body,
                                &item_ctx,
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
                let n = eval_owned(child, context, templates, styles, scope, owner, None, None)?;
                // A `Fragment` (a multi-root component template, or an explicit
                // `Fragment { … }`) is transparent: splice its already-evaluated
                // children into this list instead of pushing a wrapper node, so
                // e.g. a component that is an `if`/`else` pair renders as two
                // siblings of the surrounding layout.
                if matches!(n.kind, NodeType::Fragment) {
                    out.extend(n.children);
                } else {
                    out.push(n);
                }
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
) -> Result<UiNode> {
    // A fronteira: o motor tem um `HashMap`; a avaliação por dentro trabalha
    // sobre o [`EvalCtx`] em camadas, para não clonar a base por item de lista.
    let ctx = EvalCtx::new(context);
    eval_owned(node, &ctx, templates, styles, scope, None, None, None)
}

/// Prefixes an action with its owning component, so `dispatch` can route it.
/// Actions inside a `<Component name="X">` subtree become `X::action`.
/// Empty actions and navigation are left untouched.
/// Prefixos de ações built-in tratadas pelo próprio motor (`dispatch`) antes de
/// qualquer roteamento a componente — ver `GlacierUI::dispatch`. São globais, não
/// pertencem a componente algum, então **não** podem ser namespaceadas: senão o
/// `strip_prefix("clipboard:")`/`"open:"`/`"window:"` erra dentro de um
/// componente importado (ex.: `ServiceDetail::clipboard:foo`).
const BUILTIN_ACTION_PREFIXES: [&str; 3] = ["clipboard:", "open:", "window:"];

fn namespace_action(action: String, owner: Option<&str>) -> String {
    match owner {
        Some(name)
            if !action.is_empty()
                && !BUILTIN_ACTION_PREFIXES.iter().any(|p| action.starts_with(p)) =>
        {
            format!("{}::{}", name, action)
        }
        _ => action,
    }
}

/// Core of [`evaluate_node`]. `owner` is the name of the nearest enclosing
/// `<Component>`/`<Include>` reference, used to namespace its actions. `scope`
/// is the component whose `<link>`-scoped stylesheets are currently in effect
/// (it follows the same component boundaries as `owner`).
#[allow(clippy::too_many_arguments)]
fn eval_owned(
    node: &UiNode,
    context: &EvalCtx,
    templates: &HashMap<String, UiNode>,
    styles: &StyleContext,
    scope: Option<&str>,
    owner: Option<&str>,
    // Underlay de **tag-de-componente** (`Card {}`), passado só para a raiz
    // avaliada do template de um componente: entra como o tier de MENOR
    // especificidade (abaixo de tag builtin/classe/id/inline). `None` no caso
    // comum. Aninhamento: o componente interno recebe o do externo já mesclado.
    underlay: Option<&StyleRule>,
    underlay_states: Option<&StateStyles>,
) -> Result<UiNode> {
    // A component reference — either the legacy `<Include src="..." />` or a tag
    // named after a registered component (e.g. `<PerfilCard ... />`) — is replaced
    // with the evaluated template root, with its attributes passed in as props.
    let reference: Option<(&String, &HashMap<String, String>)> = match &node.kind {
        NodeType::Include { src, props } => Some((src, props)),
        NodeType::Component { name, props } => Some((name, props)),
        _ => None,
    };
    if let Some((name, props)) = reference {
        let template_ast = templates
            .get(name)
            .ok_or_else(|| crate::error::GlacierError::UnknownComponent(name.clone()))?;

        // As props do componente entram numa CAMADA sobre o contexto do uso (que
        // o template do componente enxerga por baixo), sem clonar a base — ver
        // [`EvalCtx`]. Uma prop de mesmo nome que uma chave global a sombreia,
        // como antes.
        let mut layer = Layer::new(context.layer());
        for (key, val_template) in props {
            layer.set(key.clone(), process_tpl(val_template, context));
        }
        let local_context = context.with(&layer);

        // Underlay de tag-de-componente: `Card {}` (minúsculo) casa o *nome* do
        // componente no seu uso. Como o componente é inlinado, o estilo é
        // resolvido aqui (sheets do escopo do USO) e passado como underlay de
        // menor especificidade para a raiz avaliada do template. Herda o
        // underlay do componente externo (aninhamento), com este por cima.
        let mut underlay_rule = underlay.cloned().unwrap_or_default();
        let mut underlay_st = underlay_states.cloned().unwrap_or_default();
        if styles.has_tag_rules {
            let active = styles.active(scope);
            let tag = name.to_lowercase();
            underlay_rule.merge_from(&resolve_classes(Some(&tag), "", None, &active, styles.viewport));
            underlay_st.merge_from(&resolve_state_classes(Some(&tag), "", None, &active, styles.viewport));
        }

        // The referenced subtree's actions and scoped styles belong to `name`
        // (innermost wins).
        return eval_owned(
            template_ast, &local_context, templates, styles, Some(name), Some(name),
            Some(&underlay_rule), Some(&underlay_st),
        );
    }

    // Resolve `class="..."` into a merged style rule that sits *underneath* the
    // node's inline attributes (inline wins, per CSS precedence). Global sheets
    // apply first, then the current component's scoped sheets. Pseudo-state
    // overlays (`.classe:hover { }` etc.) are resolved alongside the base rule
    // from the very same class list/sheets/viewport, so they stay consistent.
    // Style resolution, by ascending specificity (each overriding the previous):
    //   component-tag underlay  <  builtin-tag  <  class  <  id  <  inline
    // The underlay (from an enclosing `<Card/>`, if any) is the base; the tag
    // (this node's builtin kind), classes and id are merged on top by
    // `resolve_classes`; inline attrs win last, in the per-field match below.
    // `class`/`id` are interpolated (`id="item-{i}"` works). The `styles.active`
    // allocation is skipped for a plain node unless a tag rule is in play.
    let (style, state_styles): (StyleRule, StateStyles) = {
        let mut base = underlay.cloned().unwrap_or_default();
        let mut states = underlay_states.cloned().unwrap_or_default();
        let tag = node.kind.tag_name();
        let needs_lookup = node.class.is_some()
            || node.id.is_some()
            || (tag.is_some() && styles.has_tag_rules);
        if needs_lookup {
            let active = styles.active(scope);
            let processed = node.class.as_deref()
                .map(|c| process_tpl(c, context))
                .unwrap_or_default();
            let id = node.id.as_deref().map(|i| process_tpl(i, context));
            base.merge_from(&resolve_classes(tag, &processed, id.as_deref(), &active, styles.viewport));
            states.merge_from(&resolve_state_classes(tag, &processed, id.as_deref(), &active, styles.viewport));
        }
        (base, states)
    };

    // Resolve a numeric attribute whose XML value was a `{...}` template (see
    // `NumAttr`): interpolate against the context and parse to f32. `None` if
    // the node had no template for `attr`, or it resolved to a non-number.
    let num_template = |attr: NumAttr| -> Option<f32> {
        node.numeric_templates
            .iter()
            .find(|(a, _)| *a == attr)
            .and_then(|(_, t)| process_tpl(t, context).trim().parse::<f32>().ok())
    };

    // Evaluate current node attributes
    let kind_eval = match &node.kind {
        NodeType::Container => NodeType::Container,
        NodeType::Column => NodeType::Column,
        NodeType::Row => NodeType::Row,
        NodeType::Text { content, size, bold, color } => {
            NodeType::Text {
                content: process_tpl(content, context),
                size: num_template(NumAttr::Size).or(*size).or(style.size),
                bold: *bold || style.bold.unwrap_or(false),
                color: color.as_ref()
                    .map(|c| process_tpl(c, context))
                    .or_else(|| style.color.clone()),
            }
        }
        NodeType::Button { text, on_click, navigate_to, navigate_back, color } => {
            NodeType::Button {
                text: process_tpl(text, context),
                on_click: on_click.as_ref()
                    .map(|o| namespace_action(process_tpl(o, context), owner)),
                navigate_to: navigate_to.as_ref().map(|n| process_tpl(n, context)),
                navigate_back: *navigate_back,
                color: color.as_ref()
                    .map(|c| process_tpl(c, context))
                    .or_else(|| style.color.clone()),
            }
        }
        NodeType::TextInput { placeholder, value_var, on_change, secure } => {
            NodeType::TextInput {
                placeholder: process_tpl(placeholder, context),
                value_var: process_tpl(value_var, context),
                on_change: namespace_action(process_tpl(on_change, context), owner),
                secure: *secure,
            }
        }
        NodeType::TextArea { placeholder, value_var, on_change, readonly } => {
            NodeType::TextArea {
                placeholder: process_tpl(placeholder, context),
                value_var: process_tpl(value_var, context),
                on_change: namespace_action(process_tpl(on_change, context), owner),
                readonly: *readonly,
            }
        }
        NodeType::Image { source, clip_circle } => {
            NodeType::Image {
                source: process_tpl(source, context),
                clip_circle: *clip_circle,
            }
        }
        NodeType::Svg { source, color } => {
            NodeType::Svg {
                source: process_tpl(source, context),
                color: color.as_ref()
                    .map(|c| process_tpl(c, context))
                    .or_else(|| style.color.clone()),
            }
        }
        NodeType::Scrollable { direction } => NodeType::Scrollable { direction: direction.clone() },
        NodeType::Checkbox { label, checked_var, on_toggle } => {
            NodeType::Checkbox {
                label: process_tpl(label, context),
                checked_var: process_tpl(checked_var, context),
                on_toggle: namespace_action(process_tpl(on_toggle, context), owner),
            }
        }
        NodeType::Toggle { label, checked_var, on_toggle } => {
            NodeType::Toggle {
                label: process_tpl(label, context),
                checked_var: process_tpl(checked_var, context),
                on_toggle: namespace_action(process_tpl(on_toggle, context), owner),
            }
        }
        NodeType::Rule { horizontal } => NodeType::Rule { horizontal: *horizontal },
        NodeType::Select { options, value_var, on_change, placeholder, label_field, value_field, color } => {
            NodeType::Select {
                options: process_tpl(options, context),
                value_var: process_tpl(value_var, context),
                on_change: namespace_action(process_tpl(on_change, context), owner),
                placeholder: process_tpl(placeholder, context),
                label_field: label_field.clone(),
                value_field: value_field.clone(),
                color: color.as_ref()
                    .map(|c| process_tpl(c, context))
                    .or_else(|| style.color.clone()),
            }
        }
        NodeType::Form { on_submit, name } => {
            NodeType::Form {
                on_submit: on_submit.as_ref().map(|s| namespace_action(process_tpl(s, context), owner)),
                name: name.as_ref().map(|n| process_tpl(n, context)),
            }
        }
        // A `Fragment` carries through evaluation as-is; its children are
        // spliced into the parent by `expand_children` (below), so it stays
        // transparent instead of collapsing into a `Container` box.
        NodeType::Fragment => NodeType::Fragment,
        NodeType::Include { .. } | NodeType::Component { .. } | NodeType::Import { .. }
        | NodeType::ForEach { .. } | NodeType::If { .. } | NodeType::Else
        | NodeType::Link { .. } | NodeType::Style { .. } => {
            NodeType::Container
        }
    };

    // For each style field, the node's inline attribute wins; a `class` value
    // (if any) fills in only where the inline attribute is absent.
    let resolve = |inline: &Option<String>, class: &Option<String>| -> Option<String> {
        inline
            .as_ref()
            .map(|s| process_tpl(s, context))
            .or_else(|| class.clone())
    };

    let width_eval = resolve(&node.width, &style.width);
    let height_eval = resolve(&node.height, &style.height);
    let padding_eval = resolve(&node.padding, &style.padding);
    let align_x_eval = resolve(&node.align_x, &style.align_x);
    let align_y_eval = resolve(&node.align_y, &style.align_y);
    let background_eval = resolve(&node.background, &style.background);
    let border_color_eval = resolve(&node.border_color, &style.border_color);
    let spacing_eval = num_template(NumAttr::Spacing).or(node.spacing).or(style.spacing);
    let border_radius_eval = num_template(NumAttr::BorderRadius).or(node.border_radius).or(style.border_radius);
    let border_width_eval = num_template(NumAttr::BorderWidth).or(node.border_width).or(style.border_width);
    let font_eval = resolve(&node.font, &style.font);
    let gradient_eval = resolve(&node.gradient, &style.gradient);
    let text_align_eval = resolve(&node.text_align, &style.text_align);
    // `on_press` is behavior, not a style field; interpolate it directly so
    // actions like `onPress="window:{cmd}"` can bind context values.
    let on_press_eval = node.on_press.as_ref().map(|s| process_tpl(s, context));
    let on_double_click_eval = node.on_double_click.as_ref().map(|s| process_tpl(s, context));
    let cursor_eval = resolve(&node.cursor, &style.cursor);
    let text_color_eval = resolve(&node.text_color, &style.text_color);
    // `tooltip` é conteúdo, não estilo (sem equivalente `.classe { }`, como
    // `on_press`) — interpolado direto pra suportar `tooltip="{var}"`.
    let tooltip_eval = node.tooltip.as_ref().map(|s| process_tpl(s, context));
    let tooltip_position_eval = node.tooltip_position.clone();
    let max_width_eval = num_template(NumAttr::MaxWidth).or(node.max_width).or(style.max_width);
    let max_height_eval = num_template(NumAttr::MaxHeight).or(node.max_height).or(style.max_height);
    // `hidden` resolvido: inline vence a classe/`@media` (mesma precedência dos
    // demais campos). Consumido em `widget::render_node` (pulado no layout).
    let hidden_eval = node.hidden.or(style.hidden);
    // `disabled` só existe como atributo inline (sem equivalente `.classe { }`),
    // carregado direto, como `drag_handle`.
    let disabled_eval = node.disabled;
    // Overlays por pseudo-estado: só embrulha num `Box` quando o `.gss`
    // realmente declarou algo para aquele estado, para não pagar uma
    // alocação por nó no caso comum (nenhum `:hover`/`:focus`/etc. no sheet).
    let box_state = |r: StyleRule| -> Option<Box<StyleRule>> {
        if r == StyleRule::default() { None } else { Some(Box::new(r)) }
    };
    let hover_style_eval = box_state(state_styles.hover);
    let focus_style_eval = box_state(state_styles.focus);
    let active_style_eval = box_state(state_styles.active);
    let disabled_style_eval = box_state(state_styles.disabled);

    // Evaluate children recursively. ForEach/if/else/Import are structural:
    // they are expanded or dropped rather than rendered directly.
    let mut children_eval = Vec::new();
    expand_children(&node.children, context, templates, styles, scope, owner, &mut children_eval)?;

    // A `<Form>` hydrates every `formControl`-bound descendant (at any depth,
    // through nested Rows/Columns) with the shared scope, its evaluated
    // `onSubmit` action, and — per control, in document order — the name of
    // the next one, mirroring how a reorderable for-each hydrates its
    // `dragHandle` (see `hydrate_drag_item` below).
    if let NodeType::Form { on_submit, name } = &kind_eval {
        let form_scope = format!("{}::{}", owner.unwrap_or(""), name.as_deref().unwrap_or(""));
        let submit_action = on_submit.clone().unwrap_or_default();
        let mut order = Vec::new();
        collect_form_control_names(&children_eval, &mut order);
        hydrate_form_controls(&mut children_eval, &order, &form_scope, &submit_action);
    }

    Ok(UiNode {
        kind: kind_eval,
        children: children_eval,
        // Numeric templates are resolved into the f32 fields below; nothing left.
        numeric_templates: Vec::new(),
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
        // Classes and id are fully resolved into the fields above; nothing to
        // carry on.
        class: None,
        id: None,
        font: font_eval,
        gradient: gradient_eval,
        text_align: text_align_eval,
        on_press: on_press_eval,
        on_double_click: on_double_click_eval,
        cursor: cursor_eval,
        text_color: text_color_eval,
        tooltip: tooltip_eval,
        tooltip_position: tooltip_position_eval,
        max_width: max_width_eval,
        max_height: max_height_eval,
        hidden: hidden_eval,
        disabled: disabled_eval,
        hover_style: hover_style_eval,
        focus_style: focus_style_eval,
        active_style: active_style_eval,
        disabled_style: disabled_style_eval,
        if_cond: None,
        if_equals: None,
        if_not_equals: None,
        is_else: false,
        for_each: None,
        for_each_var: None,
        // `on_reorder`/`reorder_key` are only meaningful on a for-each node,
        // consumed (and interpolated) directly by `expand_children`'s for-each
        // handling below — nothing to carry on past evaluation.
        on_reorder: None,
        reorder_key: None,
        // `drag_handle` is a static marker (no template to resolve); carried
        // through unevaluated so a reorderable item's handle survives eval.
        drag_handle: node.drag_handle,
        // Hydrated (if at all) by the *parent* for-each's expansion, onto this
        // very node, before it reached this call — carried through as-is
        // (nothing here to interpolate; identities are already resolved).
        drag_list: node.drag_list.clone(),
        drag_item_key: node.drag_item_key.clone(),
        drag_order: node.drag_order.clone(),
        drag_on_reorder: node.drag_on_reorder.clone(),
        drag_reorder_key: node.drag_reorder_key.clone(),
        form_control: node.form_control.as_ref().map(|s| process_tpl(s, context)),
        // Hydrated (if at all) by the enclosing `<Form>`'s post-pass above, on
        // this very (already evaluated) node — carried through as a default of
        // `None` here, same as the drag_* fields are for a plain for-each item.
        form_scope: node.form_scope.clone(),
        form_submit_action: node.form_submit_action.clone(),
        form_next_focus: node.form_next_focus.clone(),
    })
}

/// Collects the `form_control` name of every node across `nodes` (a `<Form>`'s
/// already-evaluated subtree) in document order — the tab/Enter order used to
/// find each control's "next" one.
fn collect_form_control_names(nodes: &[UiNode], out: &mut Vec<String>) {
    for node in nodes {
        if let Some(name) = &node.form_control {
            out.push(name.clone());
        }
        collect_form_control_names(&node.children, out);
    }
}

/// Hydrates every `form_control`-bound node across `nodes` with the enclosing
/// `<Form>`'s `scope` (used to build a stable focus id) and evaluated
/// `on_submit` action, plus the name of the next control in `order` (`None` on
/// the last one).
fn hydrate_form_controls(nodes: &mut [UiNode], order: &[String], scope: &str, on_submit: &str) {
    for node in nodes.iter_mut() {
        if let Some(name) = &node.form_control {
            let next = order.iter()
                .position(|n| n == name)
                .and_then(|i| order.get(i + 1))
                .cloned();
            node.form_scope = Some(scope.to_string());
            node.form_submit_action = Some(on_submit.to_string());
            node.form_next_focus = next;
        }
        hydrate_form_controls(&mut node.children, order, scope, on_submit);
    }
}

fn hydrate_drag_item(
    nodes: &mut [UiNode],
    list: &str,
    key: &str,
    order: &[String],
    on_reorder: &str,
    reorder_key: &str,
) {
    for node in nodes.iter_mut() {
        node.drag_list = Some(list.to_string());
        node.drag_item_key = Some(key.to_string());
    }
    // Hydrate EVERY `dragHandle` in the item body, not just the first. An item
    // whose body branches on a directive — e.g. `if {e.__dragging} { …handle… }
    // else { …handle… }` — defines the handle once per branch. Only one branch
    // renders per item, and it may not be the first one found; stopping at the
    // first match (the old `find_handle` + `break`) left the *rendered* branch's
    // handle without drag metadata, so `DragStart` fired with no order and the
    // reorder silently did nothing.
    fn hydrate_handles(node: &mut UiNode, list: &str, key: &str, order: &[String], on_reorder: &str, reorder_key: &str) {
        if node.drag_handle {
            node.drag_list = Some(list.to_string());
            node.drag_item_key = Some(key.to_string());
            node.drag_reorder_key = Some(reorder_key.to_string());
            node.drag_order = Some(order.to_vec());
            node.drag_on_reorder = Some(on_reorder.to_string());
        }
        for c in node.children.iter_mut() {
            hydrate_handles(c, list, key, order, on_reorder, reorder_key);
        }
    }
    for node in nodes.iter_mut() {
        hydrate_handles(node, list, key, order, on_reorder, reorder_key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_action_prefixes_component_actions() {
        assert_eq!(
            namespace_action("connect".to_string(), Some("Login")),
            "Login::connect"
        );
    }

    #[test]
    fn namespace_action_leaves_top_level_actions_untouched() {
        assert_eq!(namespace_action("connect".to_string(), None), "connect");
    }

    #[test]
    fn namespace_action_never_namespaces_builtin_prefixes() {
        // Built-ins (`clipboard:`/`open:`/`window:`) são globais e resolvidos por
        // `GlacierUI::dispatch` via `strip_prefix` — se um componente importado os
        // namespaceasse (ex.: `ServiceDetail::clipboard:foo`), o strip falharia e o
        // clipboard/open/window nunca dispararia. Trava essa regressão.
        for action in ["clipboard:svc_external_url", "open:my_url", "window:close"] {
            assert_eq!(
                namespace_action(action.to_string(), Some("ServiceDetail")),
                action,
                "ação built-in não pode ser namespaceada"
            );
        }
    }

    // --- Seletor de tag (builtin + componente), fim-a-fim pelo eval ------------

    fn parse(xml: &str) -> UiNode {
        UiNode::parse_xml(xml).unwrap()
    }

    /// Avalia `xml` com `sheet` como sheet global e um mapa de componentes.
    fn eval_with(
        xml: &str,
        gss: &str,
        templates: &HashMap<String, UiNode>,
    ) -> UiNode {
        let global = vec![StyleSheet::parse(gss).unwrap()];
        let by_component: HashMap<String, Vec<StyleSheet>> = HashMap::new();
        let styles = StyleContext {
            global: &global,
            by_component: &by_component,
            viewport: None,
            has_tag_rules: global.iter().any(|s| s.has_tag_rules()),
        };
        evaluate_node(&parse(xml), &HashMap::new(), templates, &styles, None).unwrap()
    }

    #[test]
    fn builtin_tag_selector_applies_to_node() {
        // `Button { padding: 7 }` casa o kind builtin, sem class/id no nó.
        let out = eval_with(r#"<Button text="x" />"#, "Button { padding: 7; }", &HashMap::new());
        assert_eq!(out.padding.as_deref(), Some("7"));
    }

    #[test]
    fn inline_wins_over_builtin_tag() {
        let out = eval_with(
            r#"<Button text="x" padding="20" />"#,
            "Button { padding: 7; }",
            &HashMap::new(),
        );
        assert_eq!(out.padding.as_deref(), Some("20"));
    }

    #[test]
    fn component_tag_selector_underlays_inlined_root() {
        // `Card {}` casa o NOME do componente e vira underlay na raiz (Column) do
        // template inlinado. O `background` da raiz (via classe) vence o underlay,
        // mas o `padding`, que só o underlay declara, sobrevive.
        let mut templates = HashMap::new();
        templates.insert(
            "Card".to_string(),
            parse(r#"<Column class="root"><Text content="oi" /></Column>"#),
        );
        let out = eval_with(
            r#"<Card />"#,
            ".root { background: #101010; } Card { padding: 24; background: #ffffff; }",
            &templates,
        );
        // A raiz avaliada é a Column do template.
        assert!(matches!(out.kind, NodeType::Column));
        assert_eq!(out.padding.as_deref(), Some("24")); // só o underlay declara
        assert_eq!(out.background.as_deref(), Some("#101010")); // classe vence o underlay
    }

    #[test]
    fn tag_selector_ignored_without_any_tag_rule() {
        // Sem regra de tag no sheet, um nó pelado não paga resolução e nada muda.
        let out = eval_with(r#"<Button text="x" />"#, ".unused { padding: 9; }", &HashMap::new());
        assert_eq!(out.padding, None);
    }
}
