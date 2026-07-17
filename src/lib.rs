pub mod app;
pub mod builtins;
pub mod component;
pub mod daemon;
pub mod dialogs;
pub mod error;
pub mod eval;
pub mod forms;
pub mod luau;
pub mod net;
pub mod parser;
pub mod render_inputs;
pub mod stylesheet;
pub mod toasts;
pub mod tray;
pub mod widget;

/// Re-exported so a host app can depend on `glacier-ui` alone: `iced` types
/// (`Task`, `Element`, `Theme`, `Font`, ...) and `iced::application` itself
/// are reachable as `glacier_ui::iced::*` without adding `iced` as a
/// separate dependency.
pub use iced;

/// Flattened re-exports of the `iced` items a host app's `main`/`App` reach
/// for most often (window setup, layout, messaging), so they can come from
/// `glacier_ui::{..}` directly instead of a separate `use iced::{..}`.
pub use iced::{Element, Font, Point, Size, Subscription, Task, window};

pub use app::GlacierApp;
pub use component::{
    BroadcastMessage, Component, Context, ContextVar, DialogAction, Effect, EffectOutcome,
    FetchResult, Nav, Template, WindowSource, WindowSpec,
};
pub use daemon::{DaemonMessage, GlacierDaemon, WindowGeometry};
pub use dialogs::{ButtonRole, DialogButton, DialogIcon, DialogSpec};
pub use error::{Diagnostic, GlacierError, Result};
pub use eval::{
    EvalCache, StyleContext, evaluate_node, evaluate_template, normalize_bare_directives,
    process_template, strip_script,
};
pub use forms::{Form, FormBuilder, FormControl, Validator};
pub use luau::LuauComponent;
pub use parser::{NodeType, UiNode};
pub use stylesheet::{StyleRule, StyleSheet};
pub use toasts::{ToastKind, ToastSpec};
pub use tray::{
    TrayActions, TrayConfig, TrayHandle, TrayItem, TrayMsg, TrayRequest, notifications_enabled,
    set_notifications_enabled,
};
pub use widget::{EngineMessage, render_node};

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// The XML-to-UI rendering engine.
///
/// **Estado interno encapsulado**: os campos são privados e o acesso passa por
/// métodos ([`GlacierUI::context`], [`GlacierUI::evaluated`],
/// [`GlacierUI::current_screen`], …). Não é cerimônia: metade deles são caches
/// com invariantes acopladas (a árvore avaliada precisa ser jogada fora quando o
/// contexto muda; `stylesheets` e `stylesheet_paths` são paralelos e têm de
/// andar juntos), e um `pub` em cada um convida a quebrá-las de fora sem que o
/// compilador diga nada.
pub struct GlacierUI {
    /// Maps a component name (e.g. "perfil") to its XML file path
    registered_components: HashMap<String, String>,
    /// Tudo de que a avaliação depende e que o rastreamento por chave de contexto
    /// NÃO enxerga: folhas de estilo, templates parseados e viewport. Atrás de um
    /// portão que conta as mudanças, para o cache de avaliação se invalidar
    /// sozinho — ver [`render_inputs`], que explica por que isto não pode ser um
    /// punhado de campos soltos.
    inputs: render_inputs::RenderInputs,
    /// Árvores **avaliadas** (placeholders substituídos, componentes inlinados),
    /// por nome. Ao contrário de `parsed_templates`, este cache **não** guarda
    /// todos os templates registrados: guarda os que estão de fato em uso — a
    /// tela atual e os fixados por [`GlacierUI::keep_evaluated`]. Ver
    /// [`GlacierUI::reevaluate_all`] para o porquê.
    evaluated_templates: HashMap<String, UiNode>,
    /// Templates que o app quer manter avaliados além da tela atual (ver
    /// [`GlacierUI::keep_evaluated`]). Vazio no caso comum.
    pinned: std::collections::HashSet<String>,
    /// As chaves de contexto que cada árvore avaliada **lê**, com o valor que
    /// tinham na avaliação. É o que deixa [`GlacierUI::reevaluate_all`] responder
    /// "o que mudou é lido por esta tela?" e não fazer nada quando a resposta é
    /// não — o caso comum quando um snapshot do servidor mexe em dados de uma
    /// view que não está aberta.
    eval_deps: HashMap<String, eval::Deps>,
    /// Subárvores já avaliadas, reaproveitadas entre reavaliações quando nada de
    /// que dependem mudou (ver [`eval::EvalCache`]). É o que faz uma linha de log
    /// nova não reconstruir a sidebar nem as 45 linhas da tabela ao lado.
    eval_cache: eval::EvalCache,
    /// In-memory context data for state binding
    context_data: HashMap<String, String>,
    /// File modification times to support hot reloading
    file_mod_times: HashMap<String, SystemTime>,
    /// Name of the component currently shown as the active screen
    current_screen: Option<String>,
    /// Navigation history (stack of previous screens) used by `navigate_back`
    history: Vec<String>,
    /// Registered components (UI + behavior), keyed by component name.
    components: HashMap<String, Box<dyn component::Component>>,
    /// The custom `iced::Theme` loaded via `<link rel="theme">`, if any.
    /// Apps read it through [`GlacierUI::theme`].
    custom_theme: Option<iced::Theme>,
    /// Path of the loaded theme file, kept for hot-reload.
    theme_path: Option<String>,
    /// Data files loaded via `<link rel="data">`, as `(context key, path)`,
    /// kept for hot-reload.
    data_sources: Vec<(String, String)>,
    /// Stateful `text_editor` buffers for `<TextArea>` widgets, keyed by binding.
    editors: widget::EditorMap,
    /// Last text each editor pushed into the context, to tell an external context
    /// change (reload editor) from the editor's own edit (leave it alone).
    editor_synced: HashMap<String, String>,
    /// The reorderable list drag in progress, if any — outside the Context
    /// (like `editors` above) because it's transient render/interaction state,
    /// not something a host app's `Component::update` should see or persist.
    drag: Option<DragState>,
    /// The modal dialog currently in exhibition (see [`dialogs`]), if any.
    /// [`GlacierUI::render_current`] overlays it on top of the active
    /// screen; [`GlacierUI::dispatch`] clears it on a button click or a
    /// dismissible backdrop click.
    dialog: Option<dialogs::DialogSpec>,
    /// Quando o diálogo em exibição é um `confirm()` **suspensivo** da camada
    /// Lua (ver [`component::DialogAction::ShowResumable`]), guarda o
    /// `(owner, id)` da corrotina suspensa à espera da escolha. Um clique num
    /// botão (ou dismiss) retoma essa corrotina com o booleano em vez de
    /// despachar a `action` do botão como uma ação normal. `None` quando o
    /// diálogo veio de um `Component::update` Rust (roteamento por `action`,
    /// comportamento legado).
    dialog_resume: Option<(String, u64)>,
    /// Toasts currently in exhibition (see [`toasts`]), oldest first.
    /// [`GlacierUI::render_current`] overlays them on top of the active
    /// screen (and the dialog, if any); each expires on its own once
    /// [`GlacierUI::toast_subscription`]'s tick notices its `duration` has
    /// elapsed since `shown_at`, or earlier if its "×" is clicked
    /// ([`widget::EngineMessage::ToastDismiss`]).
    toasts: Vec<ActiveToast>,
    /// Monotonically increasing id handed to the next [`GlacierUI::show_toast`]
    /// call, so two toasts with identical content still have distinct
    /// identities to dismiss/expire independently.
    next_toast_id: u64,
    /// Long-lived streams (`sse`/`websocket`) a component's Lua asked to open,
    /// keyed by `(owner, id)`. [`GlacierUI::subscription`] turns each into an
    /// `iced::Subscription`, so inserting/removing here starts/stops the actual
    /// connection on the next runtime re-evaluation.
    active_streams: HashMap<(String, u64), component::StreamRequest>,
    /// Outbound command channels for live WebSocket streams, keyed the same way.
    /// Populated when a stream signals [`net::StreamEvent::Ready`]; used to
    /// deliver `conn:send`/`conn:close` to the connection task.
    stream_senders: HashMap<(String, u64), net::WsSender>,
    /// Names of the components the lib registers itself in [`GlacierUI::new`]
    /// (see [`crate::builtins`]). They live in the same name space as app
    /// components, so this set lets an explicit app `<import>` of the same name
    /// override the builtin instead of being skipped by the "already loaded"
    /// guard in [`GlacierUI::load_imports`]. A name is dropped from the set once
    /// the app overrides it.
    builtin_component_names: std::collections::HashSet<String>,
    /// Identidade única deste motor entre todos os motores do processo (um por
    /// janela no modelo daemon). Dobrada na [`net::StreamKey`] para que streams
    /// de janelas distintas não colidam como o mesmo recipe do iced. Ver
    /// [`GlacierUI::subscription`].
    engine_id: u64,
    /// Janelas novas pedidas por componentes deste motor (via
    /// [`component::Context::open_window`]), já resolvidas de
    /// [`component::WindowSource::Named`] para `File`. O daemon as consome com
    /// [`GlacierUI::take_pending_windows`] após cada `dispatch` e as abre.
    pending_windows: Vec<component::WindowSpec>,
    /// Broadcasts pedidos por componentes deste motor (via
    /// [`component::Context::broadcast`]). O daemon os consome com
    /// [`GlacierUI::take_pending_broadcasts`] após cada `dispatch` e os entrega
    /// às outras janelas.
    pending_broadcasts: Vec<component::BroadcastMessage>,
    /// `true` quando um componente deste motor pediu para fechar a própria
    /// janela (via [`component::Context::close_window`]). O daemon o consome com
    /// [`GlacierUI::take_close_requested`] após cada `dispatch` e fecha a janela.
    pending_close_self: bool,
    /// Coalescência de reavaliação para eventos de stream de alta frequência
    /// (`sse`/`websocket`). Reavaliar TODOS os templates a cada mensagem de
    /// stream é O(templates) por mensagem e, sob um stream verborrágico (ex.:
    /// logs de container a centenas de linhas/s), satura a thread da UI. Em vez
    /// disso, a cada mensagem de stream o contexto é aplicado (barato) mas a
    /// reavaliação é limitada a ~[`STREAM_REEVAL_INTERVAL`]: se o intervalo já
    /// passou, reavalia na hora; senão só marca `pending_reeval` e o resíduo é
    /// escoado no próximo tick de `ToastTick` (ver `prune_expired_toasts`
    /// call-site) ou na próxima mensagem elegível. Ações do usuário (clique,
    /// submit, navegação) NÃO passam por aqui — reavaliam sempre na hora.
    pending_reeval: bool,
    /// Instante da última reavaliação disparada por mensagem de stream, base do
    /// throttle acima. `None` = nunca (primeira mensagem reavalia na hora).
    last_stream_reeval: Option<std::time::Instant>,
}

/// Intervalo mínimo entre reavaliações disparadas por mensagens de stream
/// (`sse`/`websocket`). ~30fps: rápido o bastante para logs vivos parecerem
/// contínuos, lento o bastante para o custo de reavaliar os templates não
/// saturar a UI sob rajada. Ver [`GlacierUI::pending_reeval`].
const STREAM_REEVAL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

/// Contador global que dá a cada [`GlacierUI::new`] um `engine_id` único no
/// processo (ver [`GlacierUI::engine_id`]).
static NEXT_ENGINE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// A [`toasts::ToastSpec`] actually in exhibition: the spec plus the
/// engine-assigned identity and the instant it was shown, needed to dismiss
/// it individually and to tell when it has expired.
struct ActiveToast {
    id: u64,
    spec: toasts::ToastSpec,
    shown_at: std::time::Instant,
}

/// Reserved context key holding the reorder-key of the item currently being
/// dragged (set on `DragStart`, cleared on `DragEnd`). A reorderable list's
/// for-each expansion compares each item's key against it and injects a
/// per-item `{var}.__dragging` = `"true"`/`"false"` flag, so templates can
/// visually distinguish the grabbed row without any cursor tracking.
pub(crate) const DRAG_KEY_CONTEXT: &str = "__drag_key";

/// State of an in-progress drag-and-drop reorder (see `UiNode::drag_*` /
/// `EngineMessage::DragStart|DragHover|DragEnd`). `order` is mutated live as
/// the cursor moves over other items, so the list visually reflows as you
/// drag; `dragging` (the grabbed item's identity) stays fixed throughout.
struct DragState {
    list: String,
    reorder_key: String,
    on_reorder: String,
    order: Vec<String>,
    dragging: String,
}

impl Default for GlacierUI {
    fn default() -> Self {
        Self::new()
    }
}

impl GlacierUI {
    /// Creates a new, empty GlacierUI instance
    pub fn new() -> Self {
        let mut ui = Self {
            registered_components: HashMap::new(),
            inputs: render_inputs::RenderInputs::default(),
            evaluated_templates: HashMap::new(),
            pinned: std::collections::HashSet::new(),
            eval_deps: HashMap::new(),
            eval_cache: eval::EvalCache::default(),
            context_data: HashMap::new(),
            file_mod_times: HashMap::new(),
            current_screen: None,
            history: Vec::new(),
            components: HashMap::new(),
            custom_theme: None,
            theme_path: None,
            data_sources: Vec::new(),
            editors: HashMap::new(),
            editor_synced: HashMap::new(),
            drag: None,
            dialog: None,
            dialog_resume: None,
            toasts: Vec::new(),
            next_toast_id: 0,
            active_streams: HashMap::new(),
            stream_senders: HashMap::new(),
            builtin_component_names: std::collections::HashSet::new(),
            engine_id: NEXT_ENGINE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            pending_windows: Vec::new(),
            pending_broadcasts: Vec::new(),
            pending_close_self: false,
            pending_reeval: false,
            last_stream_reeval: None,
        };
        ui.register_builtins();
        ui
    }

    /// Registra os componentes que a lib traz embutidos (ver [`crate::builtins`])
    /// para que fiquem disponíveis por tag em qualquer template, sem o app
    /// precisar `register`á-los — é o que torna `<Badge/>` "sempre disponível",
    /// como uma primitiva.
    ///
    /// Roda dentro de [`GlacierUI::new`], antes de `self` retornar. Usa
    /// [`GlacierUI::register_one`] (sem reavaliar a cada um; a reavaliação
    /// acontece quando o app registra suas telas). Como todo builtin usa
    /// [`Template::Inline`] — XML compilado na crate — uma falha de parse é bug
    /// da própria lib, então `expect` mantém `new` infalível.
    fn register_builtins(&mut self) {
        for comp in builtins::builtin_components() {
            let name = comp.name().to_string();
            self.register_one(comp).unwrap_or_else(|e| {
                panic!("built-in component '{}' failed to register: {}", name, e)
            });
            self.builtin_component_names.insert(name);
        }
    }

    /// The current `iced::Theme`: the one loaded via `<link rel="theme">` if
    /// present, otherwise `Theme::Dark`. Wire it into your app with
    /// `iced::application(...).theme(|app| app.motor.theme())`.
    pub fn theme(&self) -> iced::Theme {
        self.custom_theme.clone().unwrap_or(iced::Theme::Dark)
    }

    // ── Acesso ao estado (os campos são privados; ver o doc do struct) ───────

    /// Todo o contexto, só leitura. Para uma chave só, [`GlacierUI::get_data`].
    pub fn context(&self) -> &HashMap<String, String> {
        &self.context_data
    }

    /// Nome da tela ativa, se houver.
    pub fn current_screen(&self) -> Option<&str> {
        self.current_screen.as_deref()
    }

    /// Pilha de navegação (telas anteriores), da mais antiga para a mais recente.
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// O diálogo modal em exibição, se houver.
    pub fn dialog(&self) -> Option<&dialogs::DialogSpec> {
        self.dialog.as_ref()
    }

    /// O tema carregado por `<link rel="theme">`, se houver. Para o tema
    /// *efetivo* (com o default), use [`GlacierUI::theme`].
    pub fn custom_theme(&self) -> Option<&iced::Theme> {
        self.custom_theme.as_ref()
    }

    /// Os `.gss` globais carregados, em ordem de prioridade crescente.
    pub fn stylesheets(&self) -> &[stylesheet::StyleSheet] {
        self.inputs.stylesheets()
    }

    /// `true` se `name` está registrado (tem template parseado).
    pub fn is_registered(&self, name: &str) -> bool {
        self.inputs.has_template(name)
    }

    /// A árvore **parseada** (não avaliada) de um componente registrado.
    pub fn parsed(&self, name: &str) -> Option<&UiNode> {
        self.inputs.template(name)
    }

    /// A árvore **avaliada** de `name` — placeholders resolvidos contra o
    /// contexto atual e componentes inlinados —, avaliando-a agora se ainda não
    /// estiver em cache (daí o `&mut self`: ver [`GlacierUI::reevaluate_all`],
    /// que só mantém avaliada a tela em uso).
    pub fn evaluated(&mut self, name: &str) -> Result<&UiNode> {
        if !self.evaluated_templates.contains_key(name) {
            self.evaluate_into_cache(name)?;
        }
        self.evaluated_templates
            .get(name)
            .ok_or_else(|| GlacierError::UnknownComponent(name.to_string()))
    }

    /// Mantém `name` avaliado a cada reavaliação, mesmo não sendo a tela atual —
    /// para o app raro que renderiza mais de um template ao mesmo tempo (ex.: um
    /// painel lateral que vive fora da tela). Sem isto, só a tela ativa é
    /// avaliada, e é assim que deve ser: ver [`GlacierUI::reevaluate_all`].
    pub fn keep_evaluated(&mut self, name: &str) {
        self.pinned.insert(name.to_string());
    }

    /// Loads (or reloads) an `.gss` stylesheet from disk and re-evaluates all
    /// templates so the new classes take effect.
    ///
    /// Stylesheets are layered in load order: a class defined in a file loaded
    /// later overrides the same class from an earlier file. Loading a path that
    /// is already loaded replaces it in place (used by hot-reload).
    pub fn load_stylesheet(&mut self, path: &str) -> Result<()> {
        self.load_global_stylesheet_file(path)?;
        self.reevaluate_all()
    }

    /// Reads, parses and installs (or replaces in place) an external `.gss`
    /// file into the global sheet set, keyed by its own path — shared by the
    /// public [`GlacierUI::load_stylesheet`] and by `<link rel="stylesheet">`
    /// encountered while processing a template's `<link>`s. Does not
    /// re-evaluate; callers batch that themselves.
    fn load_global_stylesheet_file(&mut self, path: &str) -> Result<()> {
        let content =
            std::fs::read_to_string(path).map_err(|e| GlacierError::io("stylesheet", path, e))?;
        // `parse_in`: o arquivo é a fonte, então a linha do erro é a linha dele
        // (offset 1) e o caminho vai no diagnóstico.
        let sheet = stylesheet::StyleSheet::parse_in(&content, Some(path), 1)?;

        let mod_time = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| SystemTime::now());

        self.install_global_stylesheet(path.to_string(), sheet);
        self.file_mod_times.insert(stylesheet_key(path), mod_time);
        Ok(())
    }

    /// Installs (or replaces in place, keyed by `key`) a parsed sheet into the
    /// global sheet set. `key` is a real file path for linked sheets, or a
    /// synthetic per-component key for a global inline `<style>` block (see
    /// [`inline_style_key`]) — either way it is what makes reloading replace
    /// the same slot instead of accumulating duplicates.
    fn install_global_stylesheet(&mut self, key: String, sheet: stylesheet::StyleSheet) {
        // O portão avança a época sozinho, e o cache de avaliação se descarta ao
        // percebê-la — não há o que lembrar de invalidar aqui. Ver `render_inputs`.
        self.inputs.install_stylesheet(key, sheet);
    }

    /// Sets the initial active screen, clearing any navigation history.
    ///
    /// Já avalia a tela: como só a tela ativa fica avaliada (ver
    /// [`GlacierUI::reevaluate_all`]), definir qual é ela é o gatilho natural
    /// para construí-la — e assim um `render_current()` logo depois funciona,
    /// sem o app precisar saber que existe uma reavaliação a chamar.
    pub fn set_initial_screen(&mut self, name: &str) {
        self.current_screen = Some(name.to_string());
        self.history.clear();
        let _ = self.reevaluate_all();
    }

    /// Navigates to a new screen, pushing the current one onto the history stack.
    /// Navigating to the screen already shown is a no-op (avoids duplicate history).
    pub fn navigate_to(&mut self, name: &str) {
        if let Some(current) = &self.current_screen {
            if current == name {
                return;
            }
            self.history.push(current.clone());
        }
        self.current_screen = Some(name.to_string());
        let _ = self.reevaluate_all();
    }

    /// Returns to the previous screen in the history, if any.
    pub fn navigate_back(&mut self) {
        if let Some(previous) = self.history.pop() {
            self.current_screen = Some(previous);
            let _ = self.reevaluate_all();
        }
    }

    /// Shows a modal dialog (see [`dialogs`]) on top of the active screen,
    /// from host app code rather than a [`component::Component`]'s `update`
    /// (which should use [`component::Context::show_dialog`] instead).
    /// Replaces any dialog already shown.
    pub fn show_dialog(&mut self, spec: dialogs::DialogSpec) {
        self.dialog = Some(spec);
    }

    /// Closes the dialog in exhibition, if any. Host app equivalent of
    /// [`component::Context::close_dialog`].
    pub fn close_dialog(&mut self) {
        self.dialog = None;
    }

    /// Shows a toast (see [`toasts`]) on top of the active screen, from host
    /// app code rather than a [`component::Component`]'s `update` (which
    /// should use [`component::Context::show_toast`] instead). Cumulative —
    /// does not replace any toast already shown. Returns the id, which can be
    /// passed to [`GlacierUI::dismiss_toast`] to close it early.
    pub fn show_toast(&mut self, spec: toasts::ToastSpec) -> u64 {
        let id = self.next_toast_id;
        self.next_toast_id += 1;
        self.toasts.push(ActiveToast {
            id,
            spec,
            shown_at: std::time::Instant::now(),
        });
        id
    }

    /// Closes a specific toast before its natural expiration. A no-op if
    /// `id` is not (or no longer) in exhibition.
    pub fn dismiss_toast(&mut self, id: u64) {
        self.toasts.retain(|t| t.id != id);
    }

    /// Drops every toast whose `duration` has elapsed since it was shown.
    /// Called on [`widget::EngineMessage::ToastTick`] (see
    /// [`GlacierUI::toast_subscription`]).
    fn prune_expired_toasts(&mut self) {
        let now = std::time::Instant::now();
        self.toasts
            .retain(|t| now.duration_since(t.shown_at) < t.spec.duration);
    }

    /// Renders the current active screen, with the active dialog (if any)
    /// and any active toasts overlaid on top via [`dialogs::overlay`] and
    /// [`toasts::overlay`] — toasts on top of the dialog, since they should
    /// stay visible (and dismissible) even while a modal is up.
    pub fn render_current(&self) -> Result<iced::Element<'_, EngineMessage>> {
        let name = self
            .current_screen
            .as_ref()
            .ok_or(GlacierError::NoActiveScreen)?;
        let screen = self.render(name)?;
        let with_dialog = match &self.dialog {
            Some(spec) => {
                iced::widget::stack![screen, dialogs::overlay(spec, &self.theme())].into()
            }
            None => screen,
        };
        Ok(if self.toasts.is_empty() {
            with_dialog
        } else {
            let active = self.toasts.iter().map(|t| (t.id, &t.spec));
            iced::widget::stack![with_dialog, toasts::overlay(active, &self.theme())].into()
        })
    }

    /// Registers a component from its XML file, recursively loading any
    /// components it declares via `<import name="..." from="..." />`.
    ///
    /// The single entry point for file-based components: if the template carries
    /// a `<script>` block (inline or an external `src`/`from` pointing at a
    /// `.luau` file), its **Luau** behavior is wired automatically — the Luau
    /// functions read/write the context via the global `ctx` table and each one
    /// answers the action of the same name (see [`crate::luau`]). A template
    /// without a `<script>` is UI-only. Either way there is no separate
    /// registration call for scripted vs. plain components.
    pub fn register_component(&mut self, name: &str, path: &str) -> Result<()> {
        self.register_component_inner(name, path)?;
        // Evaluate once, after the whole import graph has been loaded.
        let _ = self.reevaluate_all();
        Ok(())
    }

    /// Registers a [`Component`] that bundles its UI (template) and behavior.
    ///
    /// The engine resolves and parses the template, seeds the context with the
    /// component's initial state via [`Component::init`], and stores the
    /// component so that [`GlacierUI::dispatch`] can later route actions to it.
    pub fn register(&mut self, comp: Box<dyn component::Component>) -> Result<()> {
        self.register_one(comp)?;
        // Evaluate once, after the whole component tree has been registered.
        self.reevaluate_all()
    }

    /// Registers a single component and its `children()` recursively, without
    /// re-evaluating. Used by [`GlacierUI::register`].
    fn register_one(&mut self, comp: Box<dyn component::Component>) -> Result<()> {
        use component::Template;

        let name = comp.name().to_string();

        // (a) UI: resolve the template and feed it through the XML parse
        //     pipeline. `File` templates keep hot-reload support.
        let (markup, path) = match comp.template() {
            Template::File(path) => {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| GlacierError::io("template", &path, e))?;
                let mod_time = std::fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .unwrap_or_else(|_| SystemTime::now());
                self.registered_components
                    .insert(name.clone(), path.clone());
                self.file_mod_times.insert(name.clone(), mod_time);
                (content, Some(path))
            }
            Template::Inline(s) => (s, None),
        };

        // Parse the XML markup, with any `<script>` block stripped — its Lua
        // body is run at runtime by `LuaComponent`, not here.
        let (ast, _script) =
            parse_markup(path.as_deref(), &markup).map_err(|e| e.in_component(&name))?;
        self.inputs.insert_template(name.clone(), ast.clone());
        // An explicit registration of this name means it's no longer a lib
        // builtin (register_builtins re-adds its own names *after* this call, so
        // this stays a no-op for the builtins themselves).
        self.builtin_component_names.remove(&name);
        self.load_imports(&ast)?;
        self.process_links(&name, &ast)?;

        // (b) Behavior + (c) children: seed initial state (see
        //     `install_component`), then register each Rust `children()`
        //     recursively so their templates/behavior are available too.
        let children = comp.children();
        self.install_component(&name, comp);
        for child in children {
            self.register_one(child)?;
        }
        Ok(())
    }

    /// Runs `comp.init` — recording any streams it opens so the first
    /// `subscription()` call brings them live (other async requests from `init`
    /// aren't wired here) — and stores the component under `name` so
    /// [`GlacierUI::dispatch`] can route actions to it. Shared by the
    /// `Box<dyn Component>` path ([`GlacierUI::register_one`]) and the file-based
    /// path ([`GlacierUI::register_component_inner`], which wires a
    /// [`luau::LuauComponent`] when the template carries a `<script>`).
    fn install_component(&mut self, name: &str, mut comp: Box<dyn component::Component>) {
        let init_streams = {
            let mut ctx = component::Context::new(&mut self.context_data);
            ctx.set_viewport(self.inputs.viewport());
            comp.init(&mut ctx);
            ctx.streams
        };
        for req in init_streams {
            self.active_streams.insert((name.to_string(), req.id), req);
        }
        self.components.insert(name.to_string(), comp);
    }

    /// Routes an [`EngineMessage`] to the owning component (the one backing the
    /// active screen) and applies any navigation it requested, then
    /// re-evaluates the templates.
    ///
    /// Apps that use [`GlacierUI::register`] just forward every message here from
    /// their `update()` instead of matching on actions themselves.
    pub fn dispatch(&mut self, msg: &EngineMessage) -> iced::Task<EngineMessage> {
        // Built-in: `clipboard:<key>` copies a context value to the system
        // clipboard without involving a component.
        if let EngineMessage::UiClick(a) = msg {
            if let Some(key) = a.strip_prefix("clipboard:") {
                let value = self.context_data.get(key).cloned().unwrap_or_default();
                return iced::clipboard::write(value);
            }
            // Built-in: `open:<alvo>` abre uma URL no navegador padrão do SO, sem
            // envolver um componente. `<alvo>` é uma chave de contexto (abre o
            // valor guardado, como o `clipboard:`) ou, se não existir tal chave,
            // a própria string literal (`open:https://exemplo`). Útil para
            // fluxos OAuth: o script guarda a URL no contexto e a markup faz
            // `on_click="open:minha_url"`.
            if let Some(target) = a.strip_prefix("open:") {
                let url = self
                    .context_data
                    .get(target)
                    .cloned()
                    .unwrap_or_else(|| target.to_string());
                open_url(&url);
                return iced::Task::none();
            }
            // Built-in: `textarea_end:<binding>` rola o <textarea> de `binding`
            // até o FIM (e leva o cursor pro fim), sem envolver um componente.
            // Útil para um botão "ir ao fim" num log longo. Faz Scroll por
            // line_count (clampa no fundo) + Move(DocumentEnd).
            if let Some(binding) = a.strip_prefix("textarea_end:") {
                use iced::widget::text_editor::{Action, Motion};
                if let Some(content) = self.editors.get_mut(binding) {
                    let lines = content.line_count() as i32;
                    content.perform(Action::Scroll { lines });
                    content.perform(Action::Move(Motion::DocumentEnd));
                }
                let _ = self.reevaluate_all();
                return iced::Task::none();
            }
            // Built-in: `textarea_top:<binding>` rola o <textarea> até o TOPO (e
            // leva o cursor pro começo) — o par de `textarea_end`, para um botão
            // "ao topo". Faz Scroll por -line_count (clampa no topo) +
            // Move(DocumentStart). Útil quando não há barra de rolagem visível.
            if let Some(binding) = a.strip_prefix("textarea_top:") {
                use iced::widget::text_editor::{Action, Motion};
                if let Some(content) = self.editors.get_mut(binding) {
                    let lines = content.line_count() as i32;
                    content.perform(Action::Scroll { lines: -lines });
                    content.perform(Action::Move(Motion::DocumentStart));
                }
                let _ = self.reevaluate_all();
                return iced::Task::none();
            }
            // Built-in window controls: drive the host window without any
            // component code, so a borderless app can wire its custom titlebar
            // straight from markup — `on_click="window:close"` for the buttons,
            // `onPress="window:drag"` for the draggable region and
            // `onPress="window:resize:se"` for the edge/corner resize handles.
            // The window `Id` is resolved lazily via `latest`, keeping this
            // independent of how the app opened its window.
            if let Some(action) = a.strip_prefix("window:") {
                use iced::window;
                if let Some(dir) = action.strip_prefix("resize:") {
                    return match resize_direction(dir) {
                        Some(d) => window::latest().and_then(move |id| window::drag_resize(id, d)),
                        None => iced::Task::none(),
                    };
                }
                return match action {
                    "minimize" => window::latest().and_then(|id| window::minimize(id, true)),
                    "maximize" | "toggle_maximize" => {
                        window::latest().and_then(window::toggle_maximize)
                    }
                    "close" => window::latest().and_then(window::close),
                    "drag" => window::latest().and_then(window::drag),
                    _ => iced::Task::none(),
                };
            }
        }
        let (action, value) = match msg {
            EngineMessage::UiClick(a) => (a.as_str(), None),
            // Dialog buttons and backdrop dismissal (see `dialogs`) always
            // close the dialog first; a button click then routes its
            // `action` to the owning component exactly like a plain
            // `UiClick`.
            EngineMessage::DialogDismiss => {
                if self.dialog.as_ref().is_some_and(|d| d.dismissible) {
                    self.dialog = None;
                    // Um `confirm()` suspensivo dispensável resolve como `false`
                    // (cancelou), retomando a corrotina em vez de deixá-la presa.
                    if let Some((owner, id)) = self.dialog_resume.take() {
                        return self.run_on_owner(&owner, false, move |comp, ctx| {
                            comp.resume_dialog(id, false, ctx);
                        });
                    }
                }
                return iced::Task::none();
            }
            EngineMessage::DialogButton(action) => {
                self.dialog = None;
                // Diálogo de um `confirm()` suspensivo (ver `dialog_resume`): o
                // botão não roteia uma `action`, retoma a corrotina suspensa com
                // o booleano — confirmar → `true`, cancelar → `false`.
                if let Some((owner, id)) = self.dialog_resume.take() {
                    let confirmed = action.as_str() == dialogs::CONFIRM_YES;
                    return self.run_on_owner(&owner, false, move |comp, ctx| {
                        comp.resume_dialog(id, confirmed, ctx);
                    });
                }
                return self.route_to_owner(action, |comp, bare_action, ctx| {
                    comp.update(bare_action, None, ctx);
                });
            }
            // Toasts (see `toasts`) never route to a component: dismissing
            // one (button click or expiry tick) is purely engine-side state.
            EngineMessage::ToastDismiss(id) => {
                self.dismiss_toast(*id);
                return iced::Task::none();
            }
            EngineMessage::ToastTick => {
                self.prune_expired_toasts();
                // Escoa o resíduo da coalescência de stream (ver `pending_reeval`):
                // as últimas mensagens de uma rajada aparecem no máx. um tick após
                // ela cessar, sem custo quando não há nada pendente.
                self.flush_pending_reeval();
                return iced::Task::none();
            }
            // Tab / Shift+Tab: move focus between focusable widgets. iced's text
            // inputs don't advance focus on Tab themselves, so a global keyboard
            // listener (`tab_focus_from_event`) turns the keypress into this.
            EngineMessage::FocusNext => {
                return iced::widget::operation::focus_next::<EngineMessage>();
            }
            EngineMessage::FocusPrev => {
                return iced::widget::operation::focus_previous::<EngineMessage>();
            }
            EngineMessage::Viewport { width, height } => {
                let new = (*width, *height);
                // Só re-avalia se o novo tamanho ativa/desativa alguma `@media`
                // (senão um resize dispararia um re-eval por pixel à toa). Apps
                // sem `@media` nunca re-avaliam aqui.
                // `set_viewport` só avança a época se o novo tamanho ativa ou
                // desativa alguma `@media` — um resize de um pixel que não cruza
                // breakpoint nenhum não pode custar um cache inteiro. Apps sem
                // `@media` nunca reavaliam aqui.
                if self.inputs.set_viewport(new) {
                    let _ = self.reevaluate_all();
                }
                return iced::Task::none();
            }
            EngineMessage::UiInputChanged { action, value } => {
                (action.as_str(), Some(value.as_str()))
            }
            EngineMessage::Navigate(s) => {
                self.navigate_to(s);
                let _ = self.reevaluate_all();
                return iced::Task::none();
            }
            EngineMessage::NavigateBack => {
                self.navigate_back();
                let _ = self.reevaluate_all();
                return iced::Task::none();
            }
            EngineMessage::FileChanged(_) => {
                self.check_reload();
                return iced::Task::none();
            }
            EngineMessage::ContextPatch(pairs) => {
                for (k, v) in pairs {
                    self.context_data.insert(k.clone(), v.clone());
                }
                let _ = self.reevaluate_all();
                return iced::Task::none();
            }
            // An async effect finished: apply its data patch and, if it asked
            // for one, its toast — the same things a sync `update()` can request,
            // now reachable from a `ctx.perform` future's result.
            EngineMessage::EffectOutcome(outcome) => {
                for (k, v) in &outcome.patch {
                    self.context_data.insert(k.clone(), v.clone());
                }
                if let Some(spec) = &outcome.toast {
                    self.show_toast(spec.clone());
                }
                let _ = self.reevaluate_all();
                return iced::Task::none();
            }
            // A `fetch` requested by a component's Lua finished: hand the result
            // to that component so it can resume the suspended coroutine. The
            // resumed coroutine may itself issue more `fetch`es (chained
            // requests), which `run_on_owner` turns into further tasks.
            EngineMessage::LuauResume { owner, id, result } => {
                let owner = owner.clone();
                let id = *id;
                let result = result.clone();
                return self.run_on_owner(&owner, false, move |comp, ctx| {
                    comp.resume_fetch(id, &result, ctx);
                });
            }
            // An event from a long-lived stream (`sse`/`websocket`). `Ready`
            // just stores the outbound channel (WebSocket sends); the others are
            // routed to the owning component to invoke the Lua handler. `Closed`
            // also tears down the engine-side bookkeeping.
            EngineMessage::LuauStream { owner, id, event } => {
                let owner = owner.clone();
                let id = *id;
                use component::StreamEventKind as K;
                let (kind, data) = match event {
                    net::StreamEvent::Ready(sender) => {
                        self.stream_senders.insert((owner, id), sender.clone());
                        return iced::Task::none();
                    }
                    net::StreamEvent::Open => (K::Open, String::new()),
                    net::StreamEvent::Message(d) => (K::Message, d.clone()),
                    net::StreamEvent::Error(e) => (K::Error, e.clone()),
                    net::StreamEvent::Closed => {
                        self.stream_senders.remove(&(owner.clone(), id));
                        self.active_streams.remove(&(owner.clone(), id));
                        (K::Closed, String::new())
                    }
                };
                // Só mensagens (`Message`, alta frequência) coalescem a
                // reavaliação; Open/Error/Closed são raras e refletem na hora.
                let coalesce = matches!(kind, K::Message);
                return self.run_on_owner(&owner, coalesce, move |comp, ctx| {
                    comp.on_stream_event(id, kind, &data, ctx);
                });
            }
            // A `after(ms, fn)` requested by a component's Lua came due: hand
            // the timer id to that component so it can call the registered
            // handler. Same shape as `LuauStream`, but one-shot.
            EngineMessage::LuauTimer { owner, id } => {
                let owner = owner.clone();
                let id = *id;
                return self.run_on_owner(&owner, false, move |comp, ctx| {
                    comp.resume_timer(id, ctx);
                });
            }
            // Drag-and-drop reordering of a `for-each`/`ForEach` list (see
            // `UiNode::drag_*`). `DragStart`/`DragHover` are purely internal —
            // no component ever sees them, same as `window:*` above; only
            // `DragEnd` reaches the owning `Component`, via a synthetic
            // `UiInputChanged` carrying the final order as JSON.
            EngineMessage::DragStart {
                list,
                reorder_key,
                on_reorder,
                order,
                key,
            } => {
                self.drag = Some(DragState {
                    list: list.clone(),
                    reorder_key: reorder_key.clone(),
                    on_reorder: on_reorder.clone(),
                    order: order.clone(),
                    dragging: key.clone(),
                });
                // Publish the grabbed item's key so the reorderable list's
                // items can style themselves (`{var}.__dragging`, injected per
                // item by the for-each expansion). Re-evaluate right away so
                // the highlight shows on grab, not only after the first hover.
                self.context_data
                    .insert(DRAG_KEY_CONTEXT.to_string(), key.clone());
                let _ = self.reevaluate_all();
                return iced::Task::none();
            }
            EngineMessage::DragHover { list, key } => {
                let moved = if let Some(drag) = &mut self.drag {
                    if &drag.list == list && &drag.dragging != key {
                        let from = drag.order.iter().position(|k| k == &drag.dragging);
                        let to = drag.order.iter().position(|k| k == key);
                        if let (Some(from), Some(to)) = (from, to) {
                            let item = drag.order.remove(from);
                            drag.order.insert(to, item);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };
                if moved {
                    let drag = self.drag.as_ref().expect("checked above");
                    reorder_context_json(
                        &mut self.context_data,
                        &drag.list,
                        &drag.reorder_key,
                        &drag.order,
                    );
                    let _ = self.reevaluate_all();
                }
                return iced::Task::none();
            }
            EngineMessage::DragEnd => {
                // Clear the "what am I dragging" marker so the highlight drops
                // once the item is released (see `DragStart`).
                self.context_data.remove(DRAG_KEY_CONTEXT);
                if let Some(drag) = self.drag.take() {
                    let value =
                        serde_json::to_string(&drag.order).unwrap_or_else(|_| "[]".to_string());
                    return self.dispatch(&EngineMessage::UiInputChanged {
                        action: drag.on_reorder,
                        value,
                    });
                }
                let _ = self.reevaluate_all();
                return iced::Task::none();
            }
            EngineMessage::UiSubmit { action, next_focus } => {
                // Routed to `Component::on_form_submit`, not `update` — a
                // form's field changes (`update`, via `UiInputChanged`) and
                // its submission never compete for the same `match` arm.
                // Always fires (the component decides what to do based on its
                // own `Form::is_valid()`); if there is a next control, also
                // moves focus there.
                let submit_task = self.route_to_owner(action, |comp, bare_action, ctx| {
                    comp.on_form_submit(bare_action, ctx);
                });
                return match next_focus {
                    Some(id) => iced::Task::batch([
                        submit_task,
                        iced::widget::operation::focus::<EngineMessage>(id.clone()),
                    ]),
                    None => submit_task,
                };
            }
            EngineMessage::UiEditorAction {
                binding,
                on_change,
                action,
                readonly,
            } => {
                // Read-only (<textarea readonly>): ignora EDIÇÕES (digitar/apagar/
                // colar), mantendo navegação/seleção/scroll — o texto continua
                // selecionável, copiável e rolável, só não é alterável.
                if *readonly && matches!(action, iced::widget::text_editor::Action::Edit(_)) {
                    return iced::Task::none();
                }
                // Apply the edit to the kept editor buffer, then mirror its full
                // text into the context (and `editor_synced`, so the sync step
                // doesn't treat this as an external change).
                let seed = self.context_data.get(binding).cloned().unwrap_or_default();
                let content = self
                    .editors
                    .entry(binding.clone())
                    .or_insert_with(|| iced::widget::text_editor::Content::with_text(&seed));
                content.perform(action.clone());
                let text = content.text();
                self.context_data.insert(binding.clone(), text.clone());
                self.editor_synced.insert(binding.clone(), text.clone());
                if on_change.is_empty() {
                    let _ = self.reevaluate_all();
                    return iced::Task::none();
                }
                // Let the owning component react to the change as well.
                return self.dispatch(&EngineMessage::UiInputChanged {
                    action: on_change.clone(),
                    value: text,
                });
            }
        };

        self.route_to_owner(action, |comp, bare_action, ctx| {
            comp.update(bare_action, value, ctx);
        })
    }

    /// Resolves which component owns `action` (an action namespaced as
    /// `Child::act`, produced when a `<Component>` subtree is inlined, routes
    /// to `Child` when it has registered behavior; otherwise — and for plain,
    /// un-namespaced actions — it falls back to the active screen), then
    /// calls `route` with that component and the de-namespaced action,
    /// applies any navigation it requested, re-evaluates, and turns its
    /// requested effects into `iced::Task`s. Shared by the generic `update()`
    /// dispatch path and `on_form_submit` ([`EngineMessage::UiSubmit`]), which
    /// route to different [`component::Component`] methods but need identical
    /// owner-resolution/nav/reevaluate/effects plumbing.
    fn route_to_owner(
        &mut self,
        action: &str,
        route: impl FnOnce(&mut dyn component::Component, &str, &mut component::Context),
    ) -> iced::Task<EngineMessage> {
        let (owner, bare_action) = match action.split_once("::") {
            Some((prefix, rest)) if self.components.contains_key(prefix) => {
                (prefix.to_string(), rest.to_string())
            }
            Some((_, rest)) => match &self.current_screen {
                Some(screen) => (screen.clone(), rest.to_string()),
                None => return iced::Task::none(),
            },
            None => match &self.current_screen {
                Some(screen) => (screen.clone(), action.to_string()),
                None => return iced::Task::none(),
            },
        };

        self.run_on_owner(&owner, false, move |comp, ctx| {
            route(comp, &bare_action, ctx)
        })
    }

    /// Borrows the component named `owner`, runs `run` against it and a fresh
    /// [`component::Context`], then applies everything the context accumulated —
    /// navigation, dialog, toasts, a re-evaluation — and turns its async
    /// requests into `iced::Task`s: each [`component::Effect`] into an
    /// `EffectOutcome` task, and each `fetch` ([`component::PendingFetch`]) into
    /// an HTTP task ([`crate::net::perform`]) whose completion comes back as
    /// [`EngineMessage::LuaResume`] to resume the suspended Lua coroutine.
    /// Shared by [`GlacierUI::route_to_owner`] and the `LuaResume` path.
    /// `coalesce_reeval`: quando `true` (mensagens de stream de alta frequência),
    /// a reavaliação final é limitada por [`STREAM_REEVAL_INTERVAL`] via
    /// [`GlacierUI::request_stream_reeval`] em vez de rodar a cada chamada — a
    /// menos que o handler tenha produzido uma mudança visual imediata (nav,
    /// dialog ou toast), caso em que reavalia na hora mesmo assim.
    fn run_on_owner(
        &mut self,
        owner: &str,
        coalesce_reeval: bool,
        run: impl FnOnce(&mut dyn component::Component, &mut component::Context),
    ) -> iced::Task<EngineMessage> {
        // Disjoint per-field borrows (`components` vs `context_data`) are
        // accepted by the borrow checker when done inline like this.
        let (
            nav,
            effects,
            dialog,
            toasts,
            fetches,
            streams,
            stream_cmds,
            timers,
            windows,
            broadcasts,
            notifications,
            close_self,
            editor_appends,
        ) = if let Some(comp) = self.components.get_mut(owner) {
            let mut ctx = component::Context::new(&mut self.context_data);
            ctx.set_viewport(self.inputs.viewport());
            run(comp.as_mut(), &mut ctx);
            (
                ctx.nav,
                ctx.effects,
                ctx.dialog,
                ctx.toasts,
                ctx.fetches,
                ctx.streams,
                ctx.stream_cmds,
                ctx.timers,
                ctx.windows,
                ctx.broadcasts,
                ctx.notifications,
                ctx.close_self,
                ctx.editor_appends,
            )
        } else {
            return iced::Task::none();
        };

        // Appends incrementais a <textarea> (append_textarea): insere `text` no
        // fim do Content mantido, sem recriar (preserva scroll; O(text)), e
        // sincroniza context_data + editor_synced para `clipboard:` copiar tudo e
        // sync_editors não reconstruir depois. Ver `Context::append_textarea`.
        self.apply_editor_appends(editor_appends);

        // Broadcasts para as outras janelas e o pedido de fechar a própria: só
        // acumulados aqui; o daemon/runtime os consome (`take_pending_broadcasts`
        // / `take_close_requested`) após o `dispatch`.
        self.pending_broadcasts.extend(broadcasts);
        self.pending_close_self |= close_self;

        // Notificações nativas do SO (ver Context::notify): entregues fora da
        // thread de UI — o backend é síncrono/bloqueante. São eventos raros e
        // fire-and-forget: um thread destacado por notificação, sem realimentar
        // nada ao componente; falha ao entregar só loga (ver emit_os_notification).
        // O interruptor global da bandeja ("Disable/Enable notifications") é
        // consultado aqui: desligado, o `notify()` do componente vira no-op
        // silencioso (nada de janela de UI a mudar — só não emitimos ao SO).
        for spec in notifications {
            if crate::tray::notifications_enabled() {
                std::thread::spawn(move || emit_os_notification(spec));
            }
        }

        // Novas janelas pedidas pelo componente: resolve `Named` para o caminho
        // do arquivo (a nova janela sobe um motor independente que o carrega do
        // zero) e enfileira em `pending_windows` para o daemon abrir.
        for mut spec in windows {
            if let component::WindowSource::Named(name) = &spec.source {
                match self.registered_components.get(name) {
                    Some(path) => spec.source = component::WindowSource::File(path.clone()),
                    None => {
                        eprintln!("open_window: componente '{name}' não registrado; ignorando");
                        continue;
                    }
                }
            }
            self.pending_windows.push(spec);
        }

        // Mudança visual imediata? Nav/dialog/toast precisam refletir na hora,
        // então cancelam a coalescência (senão um toast pedido por um handler de
        // stream poderia demorar até um tick para aparecer).
        let visual_change = nav.is_some() || dialog.is_some() || !toasts.is_empty();

        match nav {
            Some(component::Nav::To(s)) => self.navigate_to(&s),
            Some(component::Nav::Back) => self.navigate_back(),
            None => {}
        }

        match dialog {
            Some(component::DialogAction::Show(spec)) => {
                self.dialog = Some(spec);
                self.dialog_resume = None;
            }
            // `confirm()` suspensivo da camada Lua: além de exibir o diálogo,
            // registra quem retomar (`owner`, `id`) quando o usuário escolher.
            Some(component::DialogAction::ShowResumable(spec, id)) => {
                self.dialog = Some(spec);
                self.dialog_resume = Some((owner.to_string(), id));
            }
            Some(component::DialogAction::Close) => {
                self.dialog = None;
                self.dialog_resume = None;
            }
            None => {}
        }

        for spec in toasts {
            self.show_toast(spec);
        }

        if coalesce_reeval && !visual_change {
            self.request_stream_reeval();
        } else {
            self.last_stream_reeval = Some(std::time::Instant::now());
            self.pending_reeval = false;
            let _ = self.reevaluate_all();
        }

        // Turn each requested effect into an iced Task whose completion feeds an
        // EffectOutcome (data patch + optional toast) back through dispatch.
        let mut tasks: Vec<iced::Task<EngineMessage>> = effects
            .into_iter()
            .map(|effect| match effect {
                component::Effect::Perform(future) => {
                    iced::Task::perform(future, EngineMessage::EffectOutcome)
                }
            })
            .collect();

        // Each `fetch` becomes an async HTTP task; its result is routed back to
        // this same component to resume the coroutine that awaited it.
        for req in fetches {
            let id = req.id;
            let owner_name = owner.to_string();
            tasks.push(iced::Task::perform(
                crate::net::perform(req),
                move |result| EngineMessage::LuauResume {
                    owner: owner_name.clone(),
                    id,
                    result,
                },
            ));
        }

        // Each `after(ms, fn)` becomes a `tokio::time::sleep` task; on firing it
        // routes back to this same component's `resume_timer` (cancellation is
        // handled Lua-side, by dropping the handler — see `LuauComponent::timers`
        // — so a cancelled id's task still fires here but is a no-op).
        for t in timers {
            let id = t.id;
            let owner_name = owner.to_string();
            let dur = std::time::Duration::from_millis(t.delay_ms);
            tasks.push(iced::Task::perform(
                async move { tokio::time::sleep(dur).await },
                move |()| EngineMessage::LuauTimer {
                    owner: owner_name.clone(),
                    id,
                },
            ));
        }

        // Newly opened streams are just recorded; `subscription()` (re-evaluated
        // by the runtime after this update) turns them into live connections.
        for req in streams {
            self.active_streams.insert((owner.to_string(), req.id), req);
        }

        // Outbound `conn:send`/`conn:close`: for a live WebSocket, hand the
        // command to its channel. A `close` on a stream without a sender (SSE,
        // read-only) can't be signalled to the task, so we stop it by dropping
        // the subscription (removing it from `active_streams`).
        for cmd in stream_cmds {
            let key = (owner.to_string(), cmd.id);
            match cmd.kind {
                component::StreamCommandKind::Send => {
                    if let Some(sender) = self.stream_senders.get_mut(&key) {
                        let _ = sender.try_send(net::WsCommand::Send(cmd.text));
                    }
                }
                component::StreamCommandKind::Close => {
                    if let Some(sender) = self.stream_senders.get_mut(&key) {
                        // WebSocket: graceful close; the task then emits `Closed`,
                        // which cleans up the sender and `active_streams`.
                        let _ = sender.try_send(net::WsCommand::Close);
                    } else {
                        self.active_streams.remove(&key);
                    }
                }
            }
        }

        iced::Task::batch(tasks)
    }

    /// Retira e devolve as janelas que os componentes deste motor pediram para
    /// abrir desde a última chamada (ver [`component::Context::open_window`]). O
    /// runner [`daemon::GlacierDaemon`] chama isto após cada `dispatch` e
    /// transforma cada [`component::WindowSpec`] numa janela real do iced.
    pub fn take_pending_windows(&mut self) -> Vec<component::WindowSpec> {
        std::mem::take(&mut self.pending_windows)
    }

    /// Retira e devolve os broadcasts que os componentes deste motor pediram para
    /// enviar desde a última chamada (ver [`component::Context::broadcast`]). O
    /// runner [`daemon::GlacierDaemon`] chama isto após cada `dispatch` e entrega
    /// cada mensagem às **outras** janelas via [`GlacierUI::deliver_broadcast`].
    pub fn take_pending_broadcasts(&mut self) -> Vec<component::BroadcastMessage> {
        std::mem::take(&mut self.pending_broadcasts)
    }

    /// Retira e devolve se um componente deste motor pediu para fechar a própria
    /// janela desde a última chamada (ver [`component::Context::close_window`]).
    /// O runner [`daemon::GlacierDaemon`] chama isto após cada `dispatch`; quando
    /// `true`, fecha a janela dona deste motor.
    pub fn take_close_requested(&mut self) -> bool {
        std::mem::take(&mut self.pending_close_self)
    }

    /// Entrega um broadcast (`event`, `payload`) ao componente da tela atual
    /// deste motor, chamando seu [`Component::on_broadcast`] (a
    /// [`crate::luau::LuauComponent`] roteia para a função Lua global
    /// `on_broadcast`). Chamado pelo runner nas janelas que **não** enviaram a
    /// mensagem. Sem tela atual, é no-op.
    pub fn deliver_broadcast(&mut self, event: &str, payload: &str) -> iced::Task<EngineMessage> {
        let Some(owner) = self.current_screen.clone() else {
            return iced::Task::none();
        };
        self.run_on_owner(&owner, false, |comp, ctx| {
            comp.on_broadcast(event, payload, ctx)
        })
    }

    /// Aggregates the [`Component::subscription`] of every registered component
    /// into a single `iced::Subscription`. Wire it into your app's
    /// `subscription(&self)` so component-owned event sources (sockets, timers)
    /// stay live; their emitted [`EngineMessage::ContextPatch`] values are just
    /// forwarded to [`GlacierUI::dispatch`].
    pub fn subscription(&self) -> iced::Subscription<EngineMessage> {
        let mut subs: Vec<_> = self.components.values().map(|c| c.subscription()).collect();
        // Nota: os listeners globais de evento (drag-end, Tab, resize→viewport)
        // NÃO ficam aqui. No modelo daemon eles são registrados uma única vez no
        // nível do daemon (ver [`daemon`]), usando o `window::Id` que o callback
        // recebe para rotear ao motor certo — se cada motor os registrasse, o
        // iced fundiria os recipes idênticos num só.
        //
        // Long-lived streams opened by components' Lua (`sse`/`websocket`): one
        // subscription each, keyed by `StreamKey` so the runtime keeps it alive
        // while it's in `active_streams` and drops it once removed. `engine_id`
        // isola os streams desta janela dos de outra rodando o mesmo componente.
        for ((owner, id), req) in &self.active_streams {
            let key = net::StreamKey {
                engine_id: self.engine_id,
                owner: owner.clone(),
                id: *id,
                kind: req.kind,
                url: req.url.clone(),
                headers: req.headers.clone(),
            };
            subs.push(iced::Subscription::run_with(key, build_stream));
        }
        iced::Subscription::batch(subs)
    }

    /// Parses and stores a component plus its imports, without re-evaluating.
    fn register_component_inner(&mut self, name: &str, path: &str) -> Result<()> {
        let content =
            std::fs::read_to_string(path).map_err(|e| GlacierError::io("template", path, e))?;

        let (ast, _script) =
            parse_markup(Some(path), &content).map_err(|e| e.in_component(name))?;

        let mod_time = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| SystemTime::now());

        self.registered_components
            .insert(name.to_string(), path.to_string());
        self.inputs.insert_template(name.to_string(), ast.clone());
        self.file_mod_times.insert(name.to_string(), mod_time);
        // An explicit registration overrides any lib builtin of this name.
        self.builtin_component_names.remove(name);

        // Recursively load components declared with `<import>`.
        self.load_imports(&ast)?;
        // Process this component's `<link>` declarations.
        self.process_links(name, &ast)?;

        // Presume Luau: if the template carries a `<script>` (inline or an
        // external `src`/`from`), wire that component's Luau behavior; otherwise
        // it stays UI-only (its actions fall back to the owning screen, exactly
        // as before). This is what unifies file-based registration — there is no
        // separate `register_luau`, and imported components can be scripted too.
        if luau::has_script(&content) {
            let comp = luau::LuauComponent::from_file(path, name)?;
            self.install_component(name, Box::new(comp));
        }

        Ok(())
    }

    /// Processes the `<link>` declarations in `ast`, dispatching on `rel`:
    /// `stylesheet` (scoped to `component`), `import`/`component` (load another
    /// template), `data` (merge JSON into the context) and `theme` (set the
    /// app theme). Re-run on hot-reload of the template, so stylesheet links
    /// are rebuilt for `component` (cleared when it declares none).
    fn process_links(&mut self, component: &str, ast: &UiNode) -> Result<()> {
        let mut links = Vec::new();
        collect_links(ast, &mut links);

        for (rel, href, name) in &links {
            match rel.as_str() {
                // Always global — same slot a Rust-side `load_stylesheet` call
                // would use, keyed by `href` so re-processing (hot-reload)
                // replaces it in place instead of accumulating duplicates.
                "stylesheet" => self.load_global_stylesheet_file(href)?,
                "import" | "component" => {
                    let comp_name = name.clone().unwrap_or_else(|| file_stem(href));
                    if !self.inputs.has_template(&comp_name) {
                        self.register_component_inner(&comp_name, href)?;
                    }
                }
                "data" => {
                    let key = name.clone().ok_or_else(|| GlacierError::Link {
                        component: component.to_string(),
                        message: format!(
                            "<link rel=\"data\" href=\"{href}\"> precisa de um atributo `as`/`name` \
                             com a chave de contexto em que os dados serão guardados"
                        ),
                    })?;
                    self.load_data_file(&key, href)?;
                }
                "theme" => self.load_theme_file(href)?,
                other => {
                    return Err(GlacierError::Link {
                        component: component.to_string(),
                        message: format!(
                            "rel=\"{other}\" (href=\"{href}\") não existe; os suportados são \
                             stylesheet, import, component, data e theme"
                        ),
                    });
                }
            }
        }

        // Inline `<style>` blocks: `scoped="true"` ones stay component-scoped
        // (rebuilt here, in document order, later rules winning on a tie);
        // the rest (the default) are promoted to the global sheet set, each
        // keyed by its position among `component`'s own inline blocks so a
        // template reload replaces them in place.
        let mut scoped_css = Vec::new();
        let mut global_css = Vec::new();
        collect_inline_styles(ast, &mut scoped_css, &mut global_css);

        // O arquivo do componente (quando ele veio de um) e a linha de cada
        // `<style>` posicionam um erro do `.gss` inline no XML que o declarou:
        // "home.xml:207", não "linha 3 de um texto que você não sabe qual é".
        let file = self.registered_components.get(component).cloned();
        let parse_inline = |css: &str, line: u32| -> Result<stylesheet::StyleSheet> {
            stylesheet::StyleSheet::parse_in(css, file.as_deref(), line)
                .map_err(|e| e.in_component(component))
        };

        let sheets = scoped_css
            .iter()
            .map(|(css, line)| parse_inline(css, *line))
            .collect::<Result<Vec<_>>>()?;
        self.inputs.set_scoped_stylesheets(component, sheets);

        for (idx, (css, line)) in global_css.iter().enumerate() {
            let sheet = parse_inline(css, *line)?;
            self.install_global_stylesheet(inline_style_key(component, idx), sheet);
        }

        Ok(())
    }

    /// Loads a JSON `data` file and merges it into the context under `key`:
    /// an object's top-level fields become `key.field`; an array or scalar is
    /// stored as `key`. Tracks the source for hot-reload.
    fn load_data_file(&mut self, key: &str, path: &str) -> Result<()> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| GlacierError::io("arquivo de dados", path, e))?;
        let value: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| GlacierError::Json {
                path: path.to_string(),
                source: e,
            })?;

        merge_json(&mut self.context_data, key, &value);

        let mod_time = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| SystemTime::now());
        self.file_mod_times.insert(data_key(path), mod_time);
        if !self.data_sources.iter().any(|(k, p)| k == key && p == path) {
            self.data_sources.push((key.to_string(), path.to_string()));
        }
        Ok(())
    }

    /// Loads a JSON palette `theme` file and sets it as the app theme. Tracks
    /// the source for hot-reload.
    fn load_theme_file(&mut self, path: &str) -> Result<()> {
        let content =
            std::fs::read_to_string(path).map_err(|e| GlacierError::io("tema", path, e))?;
        let theme = parse_theme(&content, path)?;

        let mod_time = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| SystemTime::now());
        self.file_mod_times.insert(theme_key(path), mod_time);
        self.custom_theme = Some(theme);
        self.theme_path = Some(path.to_string());
        Ok(())
    }

    /// Walks a parsed tree and registers every `<import>`ed component not yet loaded.
    fn load_imports(&mut self, node: &UiNode) -> Result<()> {
        if let NodeType::Import { name, from } = &node.kind {
            // Load if the name is free, or if it currently holds a builtin the
            // app is deliberately shadowing (an explicit `<import>` wins over a
            // lib builtin). Once overridden, the name is a normal component and
            // later imports of it are skipped as before.
            let is_builtin = self.builtin_component_names.contains(name);
            if !self.inputs.has_template(name) || is_builtin {
                let (name, from) = (name.clone(), from.clone());
                self.register_component_inner(&name, &from)?;
                self.builtin_component_names.remove(&name);
            }
        }
        for child in &node.children {
            self.load_imports(child)?;
        }
        Ok(())
    }

    /// Defines or updates a value in the state context and re-evaluates all templates
    pub fn define_data(&mut self, key: &str, value: &str) {
        self.context_data.insert(key.to_string(), value.to_string());
        let _ = self.reevaluate_all();
    }

    /// Gets a value from the state context
    pub fn get_data(&self, key: &str) -> Option<&String> {
        self.context_data.get(key)
    }

    /// Gets a mutable reference to a value in the state context.
    /// Note: if you modify values, you should call `reevaluate_all()` manually.
    pub fn get_data_mut(&mut self, key: &str) -> Option<&mut String> {
        self.context_data.get_mut(key)
    }

    /// Re-evaluates all templates with the current context and caches them
    /// Reavaliação coalescida para mensagens de stream de alta frequência:
    /// reavalia na hora se [`STREAM_REEVAL_INTERVAL`] já passou desde a última;
    /// senão só marca `pending_reeval`, para o próximo tick (ou a próxima
    /// mensagem elegível) escoar. Ver [`GlacierUI::pending_reeval`].
    fn request_stream_reeval(&mut self) {
        let now = std::time::Instant::now();
        let due = self
            .last_stream_reeval
            .is_none_or(|t| now.duration_since(t) >= STREAM_REEVAL_INTERVAL);
        if due {
            self.last_stream_reeval = Some(now);
            self.pending_reeval = false;
            let _ = self.reevaluate_all();
        } else {
            self.pending_reeval = true;
        }
    }

    /// Escoa uma reavaliação coalescida pendente (ver [`GlacierUI::pending_reeval`]),
    /// para as últimas mensagens de uma rajada de stream aparecerem mesmo depois
    /// que ela cessa. Chamado nos ticks de `ToastTick`. Sem efeito (nem custo de
    /// reavaliação) quando não há nada pendente.
    fn flush_pending_reeval(&mut self) {
        if self.pending_reeval {
            self.last_stream_reeval = Some(std::time::Instant::now());
            self.pending_reeval = false;
            let _ = self.reevaluate_all();
        }
    }

    /// Aplica os appends incrementais pedidos por `append_textarea` (ver
    /// [`crate::component::Context::append_textarea`]): insere `text` no fim do
    /// [`text_editor::Content`] do editor `binding` (criando-o se preciso), SEM
    /// recriar o buffer — preserva o scroll e custa O(text), não O(conteúdo).
    /// Sincroniza `context_data` + `editor_synced` com o texto resultante para
    /// `clipboard:<binding>` copiar tudo e [`GlacierUI::sync_editors`] não
    /// reconstruir (perdendo o insert) na próxima reavaliação.
    fn apply_editor_appends(&mut self, appends: Vec<(String, String)>) {
        use iced::widget::text_editor::{Action, Edit, Motion};
        for (binding, text) in appends {
            if text.is_empty() {
                continue;
            }
            let content = self.editors.entry(binding.clone()).or_default();
            content.perform(Action::Move(Motion::DocumentEnd));
            content.perform(Action::Edit(Edit::Paste(std::sync::Arc::new(text))));
            let full = content.text();
            self.editor_synced.insert(binding.clone(), full.clone());
            self.context_data.insert(binding, full);
        }
    }

    /// Reavalia o que está **em uso** contra o contexto atual: a tela ativa e os
    /// templates fixados com [`GlacierUI::keep_evaluated`]. Chamada depois de
    /// toda mudança de contexto, estilo, markup ou navegação.
    ///
    /// O nome ficou por compatibilidade, mas o "all" é enganoso e caro: a versão
    /// anterior avaliava **todos os templates registrados**, cada um como raiz.
    /// Como avaliar um template inlina recursivamente todos os componentes que
    /// ele usa, um app cuja tela importa 15 componentes reconstruía a árvore
    /// inteira 16 vezes a cada tecla digitada — 15 delas para árvores que
    /// ninguém renderiza, já que só a tela ativa vai para a tela. O custo era
    /// O(templates × tamanho da árvore) quando o necessário é O(tamanho da
    /// árvore).
    ///
    /// Os demais templates continuam disponíveis: [`GlacierUI::evaluated`] os
    /// avalia sob demanda e memoiza até a próxima reavaliação. O cache é
    /// **limpo** aqui (em vez de marcado como obsoleto) para que "estar no cache"
    /// signifique, sem ambiguidade, "avaliado contra o contexto de agora" — um
    /// cache que guarda árvores velhas é um render silenciosamente desatualizado
    /// esperando para acontecer.
    pub fn reevaluate_all(&mut self) -> Result<()> {
        self.sync_eval_cache();

        let names: Vec<String> = self
            .current_screen
            .iter()
            .chain(self.pinned.iter())
            .cloned()
            .collect();

        for name in &names {
            // Um nome fixado que ainda não foi registrado não é erro (o app pode
            // fixá-lo antes de carregá-lo); a tela ativa idem, durante o boot.
            if !self.inputs.has_template(name) {
                continue;
            }
            // **Nada que esta árvore lê mudou?** Então ela não pode ter mudado:
            // deixe-a exatamente como está, sem reconstruir nem clonar nada. É o
            // caso comum de um app conectado a um stream — o servidor manda um
            // snapshot mexendo em chaves de views que não estão abertas, e a tela
            // em uso não tem por que ser refeita.
            if self
                .eval_deps
                .get(name)
                .is_some_and(|deps| self.deps_hold(deps))
                && self.evaluated_templates.contains_key(name)
            {
                continue;
            }
            self.evaluate_into_cache(name)?;
        }

        // Árvores de templates que saíram de uso (mudou a tela) não devem ficar
        // ocupando memória nem serem varridas pelo `sync_editors`.
        self.evaluated_templates.retain(|k, _| names.contains(k));

        self.sync_editors();
        Ok(())
    }

    /// `true` se **todas** as dependências guardadas ainda têm o mesmo valor no
    /// contexto de agora — a pergunta "posso reaproveitar o que já está pronto?".
    fn deps_hold(&self, deps: &[(String, Option<String>)]) -> bool {
        deps.iter()
            .all(|(k, v)| self.context_data.get(k).map(String::as_str) == v.as_deref())
    }

    /// Alinha o cache de avaliação com a época dos [`render_inputs`]: se algo que
    /// o rastreamento por chave **não enxerga** mudou (folha de estilo, viewport
    /// cruzando `@media`, markup recarregado), descarta tudo o que estava pronto.
    ///
    /// Não há nada a "lembrar de chamar" aqui: quem muda esses inputs só consegue
    /// fazê-lo por um método que avança a época, e esta função confere a conta.
    /// Ver [`render_inputs`], que explica por que a versão anterior — oito
    /// lembretes espalhados pelos call-sites — era uma bomba-relógio.
    fn sync_eval_cache(&mut self) {
        if self.eval_cache.sync(self.inputs.epoch()) {
            self.eval_deps.clear();
            self.evaluated_templates.clear();
        }
    }

    /// Avalia o template `name` contra o contexto atual e o guarda no cache.
    /// Ponto único onde a avaliação acontece — [`GlacierUI::reevaluate_all`]
    /// (ansiosa, para a tela ativa) e [`GlacierUI::evaluated`] (preguiçosa, sob
    /// demanda) passam os dois por aqui.
    fn evaluate_into_cache(&mut self, name: &str) -> Result<()> {
        self.sync_eval_cache();
        let (evaluated, deps) = {
            let ast = self
                .inputs
                .template(name)
                .ok_or_else(|| GlacierError::UnknownComponent(name.to_string()))?;
            let styles = StyleContext {
                global: self.inputs.stylesheets(),
                by_component: self.inputs.component_stylesheets(),
                viewport: Some(self.inputs.viewport()),
                // Qualquer sheet (global ou de escopo) com seletor de tag liga a
                // resolução de estilo para nós sem class/id — calculado uma vez
                // aqui para não pagar por nó no caso comum (nenhum seletor de tag).
                has_tag_rules: self.inputs.has_tag_rules(),
            };
            // The template's own name is the style scope, so its `<link>`ed
            // sheets apply to its subtree.
            eval::evaluate_template(
                ast,
                &self.context_data,
                self.inputs.templates(),
                &styles,
                Some(name),
                &mut self.eval_cache,
            )?
        };
        self.evaluated_templates.insert(name.to_string(), evaluated);
        self.eval_deps.insert(name.to_string(), deps);
        Ok(())
    }

    /// `true` se mover o viewport de `old` para `new` ativa ou desativa algum
    /// bloco `@media` (global ou com escopo) — usado por `dispatch` para só
    /// re-avaliar quando o resultado das media queries realmente muda.
    #[allow(dead_code)] // mantido: `set_viewport` já decide sozinho, mas a
    // pergunta "cruzou breakpoint?" segue útil a um app que queira antecipá-la.
    fn media_set_changes(&self, old: (f32, f32), new: (f32, f32)) -> bool {
        let sheets = self
            .inputs
            .stylesheets()
            .iter()
            .chain(self.inputs.component_stylesheets().values().flatten());
        for sheet in sheets {
            for mq in &sheet.media {
                if mq.condition.matches(old.0, old.1) != mq.condition.matches(new.0, new.1) {
                    return true;
                }
            }
        }
        false
    }

    /// Keeps the stateful `<TextArea>` buffers in step with the context: creates
    /// a buffer the first time a binding appears, and reloads it when the context
    /// value changed from outside the editor (e.g. a fetch). The editor's own
    /// edits set `editor_synced`, so they are not mistaken for external changes.
    fn sync_editors(&mut self) {
        let mut bindings: Vec<String> = Vec::new();
        for ast in self.evaluated_templates.values() {
            collect_textarea_bindings(ast, &mut bindings);
        }
        for b in bindings {
            let ctx_val = self.context_data.get(&b).cloned().unwrap_or_default();
            let last = self.editor_synced.get(&b);
            if !self.editors.contains_key(&b) || last != Some(&ctx_val) {
                self.editors.insert(
                    b.clone(),
                    iced::widget::text_editor::Content::with_text(&ctx_val),
                );
                self.editor_synced.insert(b, ctx_val);
            }
        }
    }

    /// Recursively evaluates the component and translates it into an Iced Element
    /// Renderiza a árvore avaliada de `component_name` em widgets do iced.
    ///
    /// Só os templates **em uso** ficam avaliados (a tela atual e os fixados com
    /// [`GlacierUI::keep_evaluated`]) — ver [`GlacierUI::reevaluate_all`]. Pedir
    /// outro nome aqui devolve [`GlacierError::UnknownComponent`] em vez de
    /// renderizar uma árvore velha em silêncio; o `&self` (exigido pelo `view`
    /// do iced) impede avaliar na hora.
    pub fn render<'a>(&'a self, component_name: &str) -> Result<iced::Element<'a, EngineMessage>> {
        let evaluated_ast = self
            .evaluated_templates
            .get(component_name)
            .ok_or_else(|| {
                // Distingue "nome errado" de "template fora de uso": são causas
                // diferentes, com saídas diferentes, e confundi-las é o que faz um
                // erro de framework virar meia hora de depuração.
                if self.inputs.has_template(component_name) {
                    GlacierError::NotEvaluated(component_name.to_string())
                } else {
                    GlacierError::UnknownComponent(component_name.to_string())
                }
            })?;

        // Render the evaluated AST to Iced Widgets
        Ok(render_node(
            evaluated_ast,
            &self.context_data,
            &self.editors,
        ))
    }

    /// Checks registered XML files for changes and re-parses them if modified.
    /// Returns the list of component names that were reloaded.
    pub fn check_reload(&mut self) -> Vec<String> {
        let mut reloaded = Vec::new();
        let mut updates = Vec::new();

        for (name, path) in &self.registered_components {
            if let Ok(metadata) = std::fs::metadata(path)
                && let Ok(modified) = metadata.modified()
            {
                let last_modified = self.file_mod_times.get(name);
                if last_modified.is_none_or(|&last| modified > last) {
                    // File changed, reload it (XML).
                    if let Ok(content) = std::fs::read_to_string(path)
                        && let Ok((new_ast, _script)) = parse_markup(Some(path.as_str()), &content)
                    {
                        updates.push((name.clone(), new_ast, modified));
                        reloaded.push(name.clone());
                    }
                }
            }
        }

        // Detect changed `.gss` files the same way. Only global sheets carry a
        // real path to watch — inline `<style>` blocks (global or scoped) are
        // rebuilt when their declaring template reloads, in the loop below.
        // (An inline global block's synthetic key simply misses `fs::metadata`
        // and is skipped here, harmlessly.)
        let all_paths = self.inputs.stylesheet_paths().to_vec();
        let mut sheet_updates = Vec::new();
        for path in &all_paths {
            if let Ok(modified) = std::fs::metadata(path).and_then(|m| m.modified()) {
                let last_modified = self.file_mod_times.get(&stylesheet_key(path));
                if last_modified.is_none_or(|&last| modified > last)
                    && let Ok(content) = std::fs::read_to_string(path)
                {
                    match stylesheet::StyleSheet::parse(&content) {
                        Ok(sheet) => {
                            sheet_updates.push((path.clone(), sheet, modified));
                            reloaded.push(path.clone());
                        }
                        Err(e) => eprintln!(
                            "Stylesheet '{}' has an error, keeping the previous version: {}",
                            path, e
                        ),
                    }
                }
            }
        }

        let mut dirty = !updates.is_empty() || !sheet_updates.is_empty();

        // Apply XML template changes.
        for (name, new_ast, modified) in updates {
            // Pick up any newly-added `<import>`/`<link>` declarations.
            let _ = self.load_imports(&new_ast);
            let _ = self.process_links(&name, &new_ast);
            self.inputs.insert_template(name.clone(), new_ast);
            self.file_mod_times.insert(name, modified);
        }

        // Apply stylesheet changes in place, preserving load order/priority.
        // Passa pelo portão (`install_stylesheet` substitui a mesma posição pela
        // chave), que avança a época e invalida o cache. Antes isto escrevia
        // direto em `stylesheets[idx]` e só não servia estilo velho porque um
        // `invalidate` genérico vinha depois — o tipo de furo que este módulo
        // existe para tornar impossível.
        for (path, sheet, modified) in sheet_updates {
            self.inputs.install_stylesheet(path.clone(), sheet);
            self.file_mod_times.insert(stylesheet_key(&path), modified);
        }

        // Reload a changed `<link rel="data">` JSON file (re-merged into context).
        for (key, path) in self.data_sources.clone() {
            if let Ok(modified) = std::fs::metadata(&path).and_then(|m| m.modified()) {
                let changed = self
                    .file_mod_times
                    .get(&data_key(&path))
                    .is_none_or(|&last| modified > last);
                if changed {
                    match self.load_data_file(&key, &path) {
                        Ok(()) => {
                            reloaded.push(path.clone());
                            dirty = true;
                        }
                        Err(e) => eprintln!(
                            "Data file '{}' has an error, keeping the previous version: {}",
                            path, e
                        ),
                    }
                }
            }
        }

        // Reload a changed `<link rel="theme">` palette file.
        if let Some(path) = self.theme_path.clone()
            && let Ok(modified) = std::fs::metadata(&path).and_then(|m| m.modified())
        {
            let changed = self
                .file_mod_times
                .get(&theme_key(&path))
                .is_none_or(|&last| modified > last);
            if changed {
                match self.load_theme_file(&path) {
                    Ok(()) => {
                        reloaded.push(path.clone());
                        dirty = true;
                    }
                    Err(e) => eprintln!(
                        "Theme '{}' has an error, keeping the previous version: {}",
                        path, e
                    ),
                }
            }
        }

        if dirty {
            // Re-evaluate all templates against the new markup/styles/data. O
            // cache se alinha sozinho pela época (ver `sync_eval_cache`).
            let _ = self.reevaluate_all();
        }

        reloaded
    }

    /// Returns a Subscription that ticks periodically to trigger file reloading checks.
    /// The client application should map this subscription to call `check_reload`.
    pub fn reload_subscription(period: Duration) -> iced::Subscription<EngineMessage> {
        iced::time::every(period).map(|_| EngineMessage::FileChanged("".to_string()))
    }

    /// Returns a Subscription that ticks periodically to expire toasts (see
    /// [`toasts`]) whose `duration` has elapsed. Without this wired into the
    /// host app's `subscription()`, toasts are only closed by clicking their
    /// "×" — they never disappear on their own. A period around 250ms-1s is
    /// plenty; toasts are seconds-long by nature.
    pub fn toast_subscription(period: Duration) -> iced::Subscription<EngineMessage> {
        iced::time::every(period).map(|_| EngineMessage::ToastTick)
    }
}

/// Parses a template's XML source into a [`UiNode`], returning the tree and its
/// `<script>` body (if any). `path` é citado nos erros de sintaxe — as duas
/// passadas de pré-processamento aqui (tirar o `<script>`, normalizar diretivas
/// nuas) preservam a contagem de linhas de propósito, para a linha reportada ser
/// a do arquivo que o autor escreveu.
fn parse_markup(path: Option<&str>, content: &str) -> Result<(UiNode, Option<String>)> {
    let (markup, script) = eval::strip_script(content);
    let markup = eval::normalize_bare_directives(&markup);
    // `content` (e não `markup`) como fonte dos trechos: o erro deve mostrar a
    // linha que o autor escreveu, não a que o pré-processamento produziu.
    Ok((
        UiNode::parse_xml_with_source(&markup, content, path)?,
        script,
    ))
}

/// Namespaced keys under which a resource's modification time is stored in
/// `file_mod_times`, so they never collide with a component of the same name
/// (or with each other across resource kinds).
fn stylesheet_key(path: &str) -> String {
    format!("gss::{}", path)
}
/// Synthetic `stylesheet_paths`/`file_mod_times`-style key for a global inline
/// `<style>` block (no `scoped` attribute) — `idx` is its position among the
/// declaring component's own inline blocks, so re-registration (hot-reload)
/// replaces the same slot in [`GlacierUI::stylesheets`] instead of piling up
/// duplicates each time the template is re-parsed.
fn inline_style_key(component: &str, idx: usize) -> String {
    format!("inline-style::{}#{}", component, idx)
}
fn data_key(path: &str) -> String {
    format!("data::{}", path)
}
fn theme_key(path: &str) -> String {
    format!("theme::{}", path)
}

/// Collects the `value` binding of every `<TextArea>` in an evaluated tree, so
/// the engine can keep a stateful editor buffer per binding.
fn collect_textarea_bindings(node: &UiNode, out: &mut Vec<String>) {
    if let NodeType::TextArea { value_var, .. } = &node.kind
        && !value_var.is_empty()
        && !out.contains(value_var)
    {
        out.push(value_var.clone());
    }
    for child in &node.children {
        collect_textarea_bindings(child, out);
    }
}

/// The file stem of a path (`templates/perfil_card.gv` -> `perfil_card`),
/// used as the default component name for `<link rel="import">`.
fn file_stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

/// Collects a component's inline `<style>` blocks' `.gss` bodies, in document
/// order, split by their `scoped` attribute — `scoped` into `scoped_out`
/// (layered on top of the global sheets, only inside this component), the
/// rest (the default) into `global_out` (promoted to the global sheet set,
/// same as a `<link rel="stylesheet">`). Blank bodies are skipped.
/// Cada bloco vem com a **linha** do seu `<style>` no arquivo, que é o que
/// traduz "linha 3 do CSS" em "linha 207 do home.xml" ao reportar um erro de
/// GSS inline (ver [`stylesheet::parse_gss_in`]).
fn collect_inline_styles(
    node: &UiNode,
    scoped_out: &mut Vec<(String, u32)>,
    global_out: &mut Vec<(String, u32)>,
) {
    if let NodeType::Style { css, scoped, line } = &node.kind
        && !css.trim().is_empty()
    {
        if *scoped {
            scoped_out.push((css.clone(), *line));
        } else {
            global_out.push((css.clone(), *line));
        }
    }
    for child in &node.children {
        collect_inline_styles(child, scoped_out, global_out);
    }
}

/// Collects every `<link>` in a parsed tree as `(rel, href, name)`, in
/// document order. Links with an empty `href` are skipped.
fn collect_links(node: &UiNode, out: &mut Vec<(String, String, Option<String>)>) {
    if let NodeType::Link { rel, href, name } = &node.kind
        && !href.is_empty()
    {
        out.push((rel.clone(), href.clone(), name.clone()));
    }
    for child in &node.children {
        collect_links(child, out);
    }
}

/// Maps a raw left-mouse-button release to `EngineMessage::DragEnd`, ignoring
/// every other event. Used by [`GlacierUI::subscription`]'s global listener —
/// a plain `fn` (not a closure) because `iced::event::listen_with` requires one.
/// Builds the event stream for one long-lived stream subscription (see
/// [`GlacierUI::subscription`]). A plain `fn` because `Subscription::run_with`
/// requires a function pointer; everything it needs comes from the `key`. Opens
/// the SSE or WebSocket connection (via [`net`]) and tags each
/// [`net::StreamEvent`] with its `owner`/`id` as an [`EngineMessage::LuaStream`].
fn build_stream(key: &net::StreamKey) -> impl iced::futures::Stream<Item = EngineMessage> + use<> {
    use iced::futures::StreamExt;
    let owner = key.owner.clone();
    let id = key.id;
    let events = match key.kind {
        component::StreamKind::Sse => net::sse(key.url.clone(), key.headers.clone()).left_stream(),
        component::StreamKind::Ws => {
            net::websocket(key.url.clone(), key.headers.clone()).right_stream()
        }
    };
    events.map(move |event| EngineMessage::LuauStream {
        owner: owner.clone(),
        id,
        event,
    })
}

/// Abre `url` no navegador padrão do SO (best-effort, não bloqueante). Usado
/// pelo built-in de ação `open:<alvo>` (ver [`GlacierUI::dispatch`]). Silencioso
/// em falha — é uma conveniência, não um caminho crítico.
fn open_url(url: &str) {
    let url = url.trim();
    if url.is_empty() {
        return;
    }
    use std::process::{Command, Stdio};
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };
    let _ = cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

fn drag_end_from_event(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<EngineMessage> {
    match event {
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
            Some(EngineMessage::DragEnd)
        }
        _ => None,
    }
}

/// Maps Tab / Shift+Tab to focus movement between focusable widgets (see
/// [`EngineMessage::FocusNext`]). A plain `fn` because `iced::event::listen_with`
/// requires one. iced text inputs ignore Tab, so this is what makes it advance.
fn tab_focus_from_event(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<EngineMessage> {
    use iced::keyboard::{Event as Kbd, Key, key::Named};
    match event {
        iced::Event::Keyboard(Kbd::KeyPressed {
            key: Key::Named(Named::Tab),
            modifiers,
            ..
        }) => Some(if modifiers.shift() {
            EngineMessage::FocusPrev
        } else {
            EngineMessage::FocusNext
        }),
        _ => None,
    }
}

/// Maps a window `Resized` event to [`EngineMessage::Viewport`], so `@media`
/// blocks re-resolve against the new size. A plain `fn` because
/// `iced::event::listen_with` requires one.
fn viewport_from_event(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<EngineMessage> {
    match event {
        iced::Event::Window(iced::window::Event::Resized(size)) => Some(EngineMessage::Viewport {
            width: size.width,
            height: size.height,
        }),
        _ => None,
    }
}

/// Reorders the JSON array stored at `context[list]` to match `order` (a
/// sequence of `reorder_key` identities), so the list renders the live reflow
/// while a drag is in progress. Elements whose identity isn't in `order`
/// (shouldn't happen — `order` is a snapshot of the same array) are kept, in
/// their original relative order, after the ones that are.
fn reorder_context_json(
    context: &mut HashMap<String, String>,
    list: &str,
    reorder_key: &str,
    order: &[String],
) {
    let Some(json_str) = context.get(list) else {
        return;
    };
    let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(json_str)
    else {
        return;
    };

    let mut by_key: HashMap<String, serde_json::Value> = HashMap::new();
    let mut leftovers: Vec<serde_json::Value> = Vec::new();
    for item in arr {
        match item
            .get(reorder_key)
            .and_then(|v| v.as_str())
            .map(String::from)
        {
            Some(k) => {
                by_key.insert(k, item);
            }
            None => leftovers.push(item),
        }
    }
    let mut reordered: Vec<serde_json::Value> =
        order.iter().filter_map(|k| by_key.remove(k)).collect();
    reordered.extend(by_key.into_values());
    reordered.extend(leftovers);

    if let Ok(new_json) = serde_json::to_string(&serde_json::Value::Array(reordered)) {
        context.insert(list.to_string(), new_json);
    }
}

/// Merges a parsed JSON `value` into `context` under `key`:
///
/// - an object's top-level fields become `key.field` entries;
/// - an array or scalar is stored as the single entry `key`.
///
/// String values are stored verbatim; everything else as compact JSON (so a
/// nested array under `key.list` still feeds `<ForEach items="key.list">`).
fn merge_json(context: &mut HashMap<String, String>, key: &str, value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                context.insert(format!("{}.{}", key, k), json_to_string(v));
            }
        }
        other => {
            context.insert(key.to_string(), json_to_string(other));
        }
    }
}

/// Renders a JSON value as a context string: strings verbatim, everything else
/// as its compact JSON representation.
fn json_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Parses a JSON palette file into an `iced::Theme`. The object must provide
/// hex colors for `background`, `text`, `primary`, `success` and `danger`; an
/// optional `name` labels the theme.
fn parse_theme(content: &str, path: &str) -> Result<iced::Theme> {
    let value: serde_json::Value =
        serde_json::from_str(content).map_err(|e| GlacierError::Json {
            path: path.to_string(),
            source: e,
        })?;
    let bad = |message: String| GlacierError::Theme {
        path: path.to_string(),
        message,
    };
    let obj = value
        .as_object()
        .ok_or_else(|| bad("o tema precisa ser um objeto JSON de cores".to_string()))?;

    let name = obj
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("custom")
        .to_string();
    let color = |field: &str| -> Result<iced::Color> {
        let hex = obj
            .get(field)
            .and_then(|v| v.as_str())
            .ok_or_else(|| bad(format!("falta a cor '{field}'")))?;
        widget::parse_hex_color(hex)
            .ok_or_else(|| bad(format!("'{hex}' não é um hex válido para '{field}'")))
    };

    let palette = iced::theme::Palette {
        background: color("background")?,
        text: color("text")?,
        primary: color("primary")?,
        success: color("success")?,
        // `warning` was added to the palette in iced 0.14; keep it optional in
        // the theme JSON and fall back to a sensible amber when absent.
        warning: color("warning").unwrap_or_else(|_| widget::parse_hex_color("#D29922").unwrap()),
        danger: color("danger")?,
    };
    Ok(iced::Theme::custom(name, palette))
}

/// Parses a resize-handle direction token (used by the built-in
/// `window:resize:<dir>` action) into an [`iced::window::Direction`].
/// Accepts compass abbreviations (`n`,`s`,`e`,`w`,`ne`,`nw`,`se`,`sw`) and
/// their full names (`north`, `south-east`, …).
fn resize_direction(s: &str) -> Option<iced::window::Direction> {
    use iced::window::Direction::*;
    Some(match s.trim().to_ascii_lowercase().as_str() {
        "n" | "north" | "top" => North,
        "s" | "south" | "bottom" => South,
        "e" | "east" | "right" => East,
        "w" | "west" | "left" => West,
        "ne" | "northeast" | "north-east" => NorthEast,
        "nw" | "northwest" | "north-west" => NorthWest,
        "se" | "southeast" | "south-east" => SouthEast,
        "sw" | "southwest" | "south-west" => SouthWest,
        _ => return None,
    })
}

/// Emite uma notificação nativa do SO (ver [`component::NotificationSpec`]).
/// Roda num thread destacado (o backend é bloqueante) e é fire-and-forget.
///
/// No **Linux**, tenta primeiro o `notify-send` (subprocesso), caindo para o
/// `notify-rust` in-process se ele não existir. O motivo é sutil mas real: alguns
/// ambientes (observado num GNOME) **suprimem** notificações fdo enviadas
/// *in-process* por um app que tem janela — o compositor associa a notificação ao
/// app pelo PID→janela (`app_id`) e a descarta silenciosamente, mesmo com o app
/// habilitado nas configurações. Um subprocesso **sem janela** (`notify-send`)
/// não é associado a nenhum app e é exibido normalmente. Em outros SOs (Windows
/// WinRT, macOS `NSUserNotification`) o `notify-rust` in-process é o caminho certo.
fn emit_os_notification(spec: component::NotificationSpec) {
    #[cfg(target_os = "linux")]
    {
        if emit_via_notify_send(&spec) {
            return;
        }
        // notify-send ausente/falhou: cai para o notify-rust in-process.
    }
    emit_via_notify_rust(&spec);
}

/// Dispara a notificação via `notify-send` (libnotify). Devolve `true` se o
/// comando existiu e retornou sucesso; `false` (não instalado / falhou) sinaliza
/// para quem chama tentar o fallback in-process.
#[cfg(target_os = "linux")]
fn emit_via_notify_send(spec: &component::NotificationSpec) -> bool {
    // `notify-send [OPÇÕES] RESUMO [CORPO]` — o resumo é obrigatório; se só há
    // corpo, ele vira o resumo (mesma normalização do prelúdio Luau).
    let (summary, body) = if spec.title.is_empty() {
        (spec.body.as_str(), "")
    } else {
        (spec.title.as_str(), spec.body.as_str())
    };
    let mut cmd = std::process::Command::new("notify-send");
    if let Some(app) = &spec.app_name {
        cmd.arg("--app-name").arg(app);
    }
    if let Some(icon) = &spec.icon {
        cmd.arg("--icon").arg(icon);
    }
    cmd.arg(summary);
    if !body.is_empty() {
        cmd.arg(body);
    }
    match cmd.status() {
        Ok(status) => status.success(),
        Err(_) => false, // provavelmente não instalado — deixa o fallback tentar.
    }
}

/// Dispara a notificação in-process via `notify-rust` (D-Bus no Linux/BSD, WinRT
/// no Windows, `NSUserNotification` no macOS). Falha só loga.
fn emit_via_notify_rust(spec: &component::NotificationSpec) {
    let mut n = notify_rust::Notification::new();
    if !spec.title.is_empty() {
        n.summary(&spec.title);
    }
    if !spec.body.is_empty() {
        n.body(&spec.body);
    }
    if let Some(app) = &spec.app_name {
        n.appname(app);
    }
    if let Some(icon) = &spec.icon {
        n.icon(icon);
    }
    if let Err(e) = n.show() {
        eprintln!("notify: falha ao emitir notificação do SO: {e}");
    }
}

/// O cache de avaliação é a única parte do motor que pode produzir uma UI
/// **silenciosamente desatualizada** — o pior tipo de bug daqui. Estes testes
/// atacam exatamente isso: cada um muda alguma coisa e exige que a árvore reflita
/// a mudança. Passar nos testes de funcionalidade não bastaria: uma árvore velha
/// servida do cache é, por definição, uma árvore que já esteve certa.
#[cfg(test)]
mod dirty_tracking_tests {
    use super::*;
    use crate::component::{Component, Context, Template};

    /// Tela com um COMPONENTE (a fronteira de cache que representa a sidebar) e
    /// uma LISTA (a outra fronteira), mais uma chave que ninguém lê — para
    /// simular a linha de log chegando pelo SSE.
    struct Tela;
    impl Component for Tela {
        fn name(&self) -> &str {
            "tela"
        }
        fn template(&self) -> Template {
            Template::Inline(
                r#"<Column>
                     <Cartao rotulo="{titulo}" />
                     <Text content="titulo: {titulo}" />
                     <Column for-each="linhas" var="l">
                       <Text content="{l.nome}" />
                     </Column>
                   </Column>"#
                    .to_string(),
            )
        }
        fn update(&mut self, _a: &str, _v: Option<&str>, _c: &mut Context) {}
        fn children(&self) -> Vec<Box<dyn Component>> {
            vec![Box::new(Cartao)]
        }
    }

    /// O componente usado pela tela — sua subárvore é memoizada pelas props.
    struct Cartao;
    impl Component for Cartao {
        fn name(&self) -> &str {
            "Cartao"
        }
        fn template(&self) -> Template {
            Template::Inline(r#"<Column><Text content="cartao: {rotulo}" /></Column>"#.to_string())
        }
        fn update(&mut self, _a: &str, _v: Option<&str>, _c: &mut Context) {}
    }

    fn textos(node: &UiNode, out: &mut Vec<String>) {
        if let NodeType::Text { content, .. } = &node.kind {
            out.push(content.clone());
        }
        for c in &node.children {
            textos(c, out);
        }
    }

    fn tela_com(dados: &[(&str, &str)]) -> GlacierUI {
        let mut m = GlacierUI::new();
        m.register(Box::new(Tela)).unwrap();
        m.set_initial_screen("tela");
        for (k, v) in dados {
            m.define_data(k, v);
        }
        m
    }

    fn conteudo(m: &mut GlacierUI) -> Vec<String> {
        let mut out = Vec::new();
        textos(m.evaluated("tela").unwrap(), &mut out);
        out
    }

    // 1. Mudar uma chave que a tela LÊ tem de refletir. É o teste que pega o
    //    cache servindo uma árvore velha.
    #[test]
    fn mudanca_em_chave_lida_reflete() {
        let mut m = tela_com(&[("titulo", "antes"), ("linhas", "[]")]);
        assert!(conteudo(&mut m).contains(&"titulo: antes".to_string()));

        m.define_data("titulo", "depois");
        let c = conteudo(&mut m);
        assert!(
            c.contains(&"titulo: depois".to_string()),
            "o cache serviu a árvore velha: {c:?}"
        );
        // E a mudança tem de atravessar a fronteira do componente: `titulo` chega
        // lá dentro como a prop `rotulo`, então a subárvore memoizada dele
        // precisa ter sido invalidada pela mudança da PROP, não da chave.
        assert!(
            c.contains(&"cartao: depois".to_string()),
            "o componente ficou com a prop velha: {c:?}"
        );
    }

    // 2. Mudar um ITEM da lista reflete só naquele item — e reflete de verdade.
    #[test]
    fn mudanca_num_item_da_lista_reflete() {
        let mut m = tela_com(&[
            ("titulo", "t"),
            ("linhas", r#"[{"nome":"a"},{"nome":"b"}]"#),
        ]);
        assert!(conteudo(&mut m).contains(&"a".to_string()));

        m.define_data("linhas", r#"[{"nome":"a"},{"nome":"B!"}]"#);
        let c = conteudo(&mut m);
        assert!(
            c.contains(&"a".to_string()),
            "o item intacto deve continuar lá: {c:?}"
        );
        assert!(
            c.contains(&"B!".to_string()),
            "o item alterado deve refletir: {c:?}"
        );
        assert!(
            !c.contains(&"b".to_string()),
            "o valor velho não pode sobreviver: {c:?}"
        );
    }

    // 3. Remover um item o tira da árvore (e a entrada órfã do cache é varrida,
    //    senão o item removido reapareceria — ou o cache cresceria sem fim).
    #[test]
    fn item_removido_some_da_arvore() {
        let mut m = tela_com(&[
            ("titulo", "t"),
            ("linhas", r#"[{"nome":"a"},{"nome":"b"}]"#),
        ]);
        assert!(conteudo(&mut m).contains(&"b".to_string()));

        m.define_data("linhas", r#"[{"nome":"a"}]"#);
        let c = conteudo(&mut m);
        assert!(
            !c.contains(&"b".to_string()),
            "o item removido continuou na árvore: {c:?}"
        );
    }

    // 4. O ganho: mudar uma chave que NINGUÉM lê não reconstrói nada. Se este
    //    passar mas os de cima falharem, o cache está rápido e errado; é a
    //    combinação que importa.
    #[test]
    fn chave_nao_lida_nao_reconstroi_a_arvore() {
        let mut m = tela_com(&[("titulo", "t"), ("linhas", r#"[{"nome":"a"}]"#)]);
        // Identidade da árvore antes: os `node_id` são preservados por clone, mas
        // uma reconstrução gera um `UiNode` novo — comparamos o ponteiro.
        let antes = m.evaluated_templates.get("tela").unwrap() as *const UiNode;

        m.define_data("__ninguem_le_isso", "1");

        let depois = m.evaluated_templates.get("tela").unwrap() as *const UiNode;
        assert_eq!(
            antes, depois,
            "a árvore foi reconstruída à toa: nada que a tela lê mudou"
        );
    }

    // 5. A variável de item (`{l.nome}`) NÃO pode virar dependência do template.
    //    Ela só existe na camada do item, nunca no contexto — se subisse, o motor
    //    perguntaria "o contexto ainda tem `l.nome` = a?", ouviria "não" para
    //    sempre, e a tela ficaria eternamente suja: o cache existiria e nunca
    //    acertaria. Foi exatamente o bug que a medição pegou.
    #[test]
    fn var_de_item_nao_suja_o_template_para_sempre() {
        let mut m = tela_com(&[
            ("titulo", "t"),
            ("linhas", r#"[{"nome":"a"},{"nome":"b"}]"#),
        ]);
        let antes = m.evaluated_templates.get("tela").unwrap() as *const UiNode;

        // Uma chave que ninguém lê. A tela tem uma lista, logo variáveis de item.
        m.define_data("__ninguem_le_isso", "1");

        let depois = m.evaluated_templates.get("tela").unwrap() as *const UiNode;
        assert_eq!(
            antes, depois,
            "a tela com lista foi reconstruída à toa — a var de item vazou para as \
             dependências do template e o deixou permanentemente sujo"
        );
    }

    // 6. Recarregar o ESTILO invalida o cache — o rastreamento só enxerga chaves
    //    de contexto, então um `.gss` novo com o cache quente serviria os nós com
    //    o estilo velho. É a armadilha mais fácil de errar neste desenho.
    #[test]
    fn estilo_novo_invalida_o_cache() {
        let dir = std::env::temp_dir().join(format!("glacier_cache_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let gss = dir.join("t.gss");
        std::fs::write(&gss, ".alvo { padding: 4; }").unwrap();

        let mut m = GlacierUI::new();
        m.load_stylesheet(gss.to_str().unwrap()).unwrap();
        m.register(Box::new(TelaClasse)).unwrap();
        m.set_initial_screen("tela_classe");
        assert_eq!(
            m.evaluated("tela_classe").unwrap().padding.as_deref(),
            Some("4")
        );

        // Mesmo template, mesmo contexto — só o estilo mudou.
        std::fs::write(&gss, ".alvo { padding: 99; }").unwrap();
        m.load_stylesheet(gss.to_str().unwrap()).unwrap();
        assert_eq!(
            m.evaluated("tela_classe").unwrap().padding.as_deref(),
            Some("99"),
            "o cache serviu o nó com o estilo velho"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    // 7. A época dos `RenderInputs` é o que agora garante a invalidação — e ela
    //    só avança quando algo que a avaliação enxerga muda. Um resize que NÃO
    //    cruza `@media` nenhuma não pode custar um cache inteiro (senão arrastar
    //    a borda da janela reconstruiria a UI a cada pixel).
    #[test]
    fn epoca_avanca_no_estilo_e_nao_num_resize_inocuo() {
        use crate::render_inputs::RenderInputs;

        let mut inputs = RenderInputs::default();
        let e0 = inputs.epoch();

        // Sem `@media` no sheet, um resize não muda estilo nenhum.
        assert!(
            !inputs.set_viewport((800.0, 600.0)),
            "resize sem @media não cruza nada"
        );
        assert_eq!(
            inputs.epoch(),
            e0,
            "resize inócuo não pode invalidar o cache"
        );

        // Uma folha nova muda o estilo de qualquer nó: a época avança.
        inputs.install_stylesheet(
            "app.gss".into(),
            stylesheet::StyleSheet::parse("@media (max-width: 500) { .a { padding: 1; } }")
                .unwrap(),
        );
        let e1 = inputs.epoch();
        assert!(e1 > e0, "instalar folha tem de avançar a época");

        // Agora HÁ uma @media em 500: encolher para 400 cruza o breakpoint.
        assert!(inputs.set_viewport((400.0, 600.0)), "cruzou o breakpoint");
        assert!(inputs.epoch() > e1, "cruzar @media tem de avançar a época");

        // Mas mover de 400 para 450 (ambos abaixo de 500) não muda nada.
        let e2 = inputs.epoch();
        assert!(!inputs.set_viewport((450.0, 600.0)));
        assert_eq!(
            inputs.epoch(),
            e2,
            "resize dentro da mesma faixa não invalida"
        );
    }

    struct TelaClasse;
    impl Component for TelaClasse {
        fn name(&self) -> &str {
            "tela_classe"
        }
        fn template(&self) -> Template {
            Template::Inline(r#"<Column class="alvo"><Text content="x" /></Column>"#.to_string())
        }
        fn update(&mut self, _a: &str, _v: Option<&str>, _c: &mut Context) {}
    }
}

#[cfg(test)]
mod scoped_eval_tests {
    use super::*;
    use crate::component::{Component, Context, Template};

    /// Componente de tela mínimo, para montar um motor com vários templates
    /// registrados e observar quais deles a reavaliação de fato constrói.
    struct Tela(&'static str);
    impl Component for Tela {
        fn name(&self) -> &str {
            self.0
        }
        fn template(&self) -> Template {
            Template::Inline("<Text content=\"{msg}\" />".to_string())
        }
        fn update(&mut self, _a: &str, _v: Option<&str>, _c: &mut Context) {}
    }

    // O ponto da avaliação escopada: registrar 3 telas e ativar 1 deve construir
    // UMA árvore, não 3. Antes, cada mudança de contexto reconstruía todas —
    // inclusive as que ninguém renderiza.
    #[test]
    fn so_a_tela_ativa_e_avaliada() {
        let mut motor = GlacierUI::new();
        motor.register(Box::new(Tela("a"))).unwrap();
        motor.register(Box::new(Tela("b"))).unwrap();
        motor.register(Box::new(Tela("c"))).unwrap();
        motor.set_initial_screen("b");
        motor.define_data("msg", "oi");

        assert_eq!(
            motor.evaluated_templates.keys().collect::<Vec<_>>(),
            vec!["b"],
            "só a tela ativa deve estar avaliada"
        );
        assert!(motor.render("b").is_ok());
    }

    // Os demais continuam alcançáveis: `evaluated()` avalia sob demanda.
    #[test]
    fn os_demais_sao_avaliados_sob_demanda() {
        let mut motor = GlacierUI::new();
        motor.register(Box::new(Tela("a"))).unwrap();
        motor.register(Box::new(Tela("b"))).unwrap();
        motor.set_initial_screen("a");
        motor.define_data("msg", "oi");

        assert!(
            motor.evaluated("b").is_ok(),
            "avaliado na hora que foi pedido"
        );
        // E o cache é invalidado por uma mudança de contexto, para não servir
        // uma árvore velha depois.
        motor.define_data("msg", "tchau");
        assert!(!motor.evaluated_templates.contains_key("b"));
    }

    // `keep_evaluated` mantém um template avaliado junto com a tela — o escape
    // hatch para o app que renderiza mais de uma árvore ao mesmo tempo.
    #[test]
    fn keep_evaluated_fixa_um_template() {
        let mut motor = GlacierUI::new();
        motor.register(Box::new(Tela("a"))).unwrap();
        motor.register(Box::new(Tela("b"))).unwrap();
        motor.keep_evaluated("b");
        motor.set_initial_screen("a");
        motor.define_data("msg", "oi");

        assert!(motor.render("a").is_ok());
        assert!(
            motor.render("b").is_ok(),
            "o fixado sobrevive à reavaliação"
        );
    }

    // Renderizar um template registrado mas fora de uso não devolve uma árvore
    // velha em silêncio — devolve um erro que diz como sair.
    #[test]
    fn render_de_template_fora_de_uso_explica_a_saida() {
        let mut motor = GlacierUI::new();
        motor.register(Box::new(Tela("a"))).unwrap();
        motor.register(Box::new(Tela("b"))).unwrap();
        motor.set_initial_screen("a");

        // `Element` não é Debug, então `unwrap_err()` não serve — casamos o Err.
        let Err(err) = motor.render("b") else {
            panic!("deveria falhar")
        };
        assert!(matches!(err, GlacierError::NotEvaluated(_)), "{err}");
        assert!(err.to_string().contains("keep_evaluated"), "{err}");

        // Nome que não existe é outro erro (outra causa, outra saída).
        let Err(err) = motor.render("nao_existe") else {
            panic!("deveria falhar")
        };
        assert!(matches!(err, GlacierError::UnknownComponent(_)), "{err}");
    }
}

#[cfg(test)]
mod editor_append_tests {
    use super::*;

    // `append_textarea` (via apply_editor_appends) cria o buffer no 1º append e,
    // nos seguintes, INSERE no fim sem substituir — as linhas coexistem em ordem.
    // E sincroniza context_data (p/ clipboard) == editor_synced (p/ sync_editors
    // não reconstruir e perder o insert).
    #[test]
    fn append_creates_and_grows_without_replacing() {
        let mut motor = GlacierUI::new();

        motor.apply_editor_appends(vec![("logs".to_string(), "linha 1\n".to_string())]);
        assert!(
            motor
                .get_data("logs")
                .is_some_and(|s| s.contains("linha 1")),
            "1º append deve refletir no ctx"
        );

        motor.apply_editor_appends(vec![("logs".to_string(), "linha 2\n".to_string())]);
        let after = motor.get_data("logs").cloned().unwrap_or_default();
        let p1 = after.find("linha 1");
        let p2 = after.find("linha 2");
        assert!(
            p1.is_some() && p2.is_some(),
            "ambas as linhas devem existir: {after:?}"
        );
        assert!(p1 < p2, "linha 2 deve vir depois da linha 1");

        // ctx == editor_synced → a próxima reavaliação não reconstrói o Content.
        assert_eq!(motor.editor_synced.get("logs"), motor.get_data("logs"));
    }
}

#[cfg(test)]
mod effect_outcome_tests {
    use super::*;
    use crate::component::EffectOutcome;
    use crate::toasts::{ToastKind, ToastSpec};

    // The constructors/builder keep `perform` ergonomic: data-only, toast-only,
    // and data-plus-toast all read cleanly.
    #[test]
    fn constructors_cover_data_and_toast() {
        let only_data = EffectOutcome::data(vec![("a".to_string(), "1".to_string())]);
        assert_eq!(only_data.patch, vec![("a".to_string(), "1".to_string())]);
        assert!(only_data.toast.is_none());

        let only_toast = EffectOutcome::toast(ToastSpec::success("done"));
        assert!(only_toast.patch.is_empty());
        assert_eq!(
            only_toast.toast.as_ref().map(|t| t.kind),
            Some(ToastKind::Success)
        );

        let both = EffectOutcome::data(vec![("k".to_string(), "v".to_string())])
            .with_toast(ToastSpec::error("boom"));
        assert_eq!(both.patch, vec![("k".to_string(), "v".to_string())]);
        assert_eq!(both.toast.as_ref().map(|t| t.kind), Some(ToastKind::Error));
    }

    // Dispatching an EffectOutcome applies its data patch *and* shows its toast,
    // the same things a sync `update()` could request — no reserved context keys.
    #[test]
    fn dispatch_applies_patch_and_toast() {
        let mut motor = GlacierUI::new();
        let outcome = EffectOutcome {
            patch: vec![("status".to_string(), "ok".to_string())],
            toast: Some(ToastSpec::success("saved")),
        };
        let _ = motor.dispatch(&EngineMessage::EffectOutcome(outcome));

        assert_eq!(motor.get_data("status"), Some(&"ok".to_string()));
        assert_eq!(motor.toasts.len(), 1, "the effect's toast should be shown");
        assert_eq!(motor.toasts[0].spec.message, "saved");
    }

    // A data-only outcome patches context without ever touching the toast list.
    #[test]
    fn dispatch_data_only_shows_no_toast() {
        let mut motor = GlacierUI::new();
        let outcome = EffectOutcome::data(vec![("n".to_string(), "42".to_string())]);
        let _ = motor.dispatch(&EngineMessage::EffectOutcome(outcome));

        assert_eq!(motor.get_data("n"), Some(&"42".to_string()));
        assert!(motor.toasts.is_empty(), "no toast requested");
    }
}
