pub mod app;
pub mod builtins;
pub mod parser;
pub mod eval;
pub mod widget;
pub mod component;
pub mod luau;
pub mod net;
pub mod stylesheet;
pub mod forms;
pub mod dialogs;
pub mod toasts;

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
pub use parser::{UiNode, NodeType};
pub use eval::{evaluate_node, process_template, strip_script, normalize_bare_directives, StyleContext};
pub use widget::{render_node, EngineMessage};
pub use component::{Component, Context, ContextVar, DialogAction, Effect, EffectOutcome, FetchResult, Nav, Template};
pub use luau::LuauComponent;
pub use stylesheet::{StyleSheet, StyleRule};
pub use forms::{Form, FormBuilder, FormControl, Validator};
pub use dialogs::{ButtonRole, DialogButton, DialogIcon, DialogSpec};
pub use toasts::{ToastKind, ToastSpec};

use std::collections::HashMap;
use std::time::{SystemTime, Duration};

/// The XML-to-UI rendering engine
pub struct GlacierUI {
    /// Maps a component name (e.g. "perfil") to its XML file path
    pub registered_components: HashMap<String, String>,
    /// Cache of parsed component AST trees
    pub parsed_templates: HashMap<String, UiNode>,
    /// Cache of fully evaluated component AST trees (placeholders substituted, includes resolved)
    pub evaluated_templates: HashMap<String, UiNode>,
    /// In-memory context data for state binding
    pub context_data: HashMap<String, String>,
    /// File modification times to support hot reloading
    pub file_mod_times: HashMap<String, SystemTime>,
    /// Name of the component currently shown as the active screen
    pub current_screen: Option<String>,
    /// Navigation history (stack of previous screens) used by `navigate_back`
    pub history: Vec<String>,
    /// Registered components (UI + behavior), keyed by component name.
    pub components: HashMap<String, Box<dyn component::Component>>,
    /// Globally-loaded `.gss` stylesheets, in ascending priority order (a class
    /// defined in a later sheet overrides the same class in an earlier one).
    pub stylesheets: Vec<stylesheet::StyleSheet>,
    /// Paths of loaded global `.gss` files (parallel to `stylesheets`), kept for
    /// hot-reload along with their last-seen modification times.
    pub stylesheet_paths: Vec<String>,
    /// Per-component (scoped) stylesheets declared via an inline
    /// `<style scoped="true">` block, keyed by component name. Applied on top
    /// of the global sheets, but only inside that component's subtree, in
    /// document order. There is no scoped equivalent for a linked `.gss` file
    /// — `<link rel="stylesheet">` is always global (see [`GlacierUI::load_stylesheet`]).
    /// Rebuilt from the markup whenever the declaring template reloads, so it
    /// needs no separate path/mtime bookkeeping of its own.
    pub component_stylesheets: HashMap<String, Vec<stylesheet::StyleSheet>>,
    /// The custom `iced::Theme` loaded via `<link rel="theme">`, if any.
    /// Apps read it through [`GlacierUI::theme`].
    pub custom_theme: Option<iced::Theme>,
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
    pub dialog: Option<dialogs::DialogSpec>,
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
    /// Current viewport size `(width, height)` in logical px, used to evaluate
    /// `@media` blocks in the stylesheets. Fed by [`EngineMessage::Viewport`]
    /// (see [`GlacierUI::subscription`]'s window-resize listener) and defaulted
    /// to a desktop-ish size until the first real `Resized` event arrives.
    viewport: (f32, f32),
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
}

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

impl GlacierUI {
    /// Creates a new, empty GlacierUI instance
    pub fn new() -> Self {
        let mut ui = Self {
            registered_components: HashMap::new(),
            parsed_templates: HashMap::new(),
            evaluated_templates: HashMap::new(),
            context_data: HashMap::new(),
            file_mod_times: HashMap::new(),
            current_screen: None,
            history: Vec::new(),
            components: HashMap::new(),
            stylesheets: Vec::new(),
            stylesheet_paths: Vec::new(),
            component_stylesheets: HashMap::new(),
            custom_theme: None,
            theme_path: None,
            data_sources: Vec::new(),
            editors: HashMap::new(),
            editor_synced: HashMap::new(),
            drag: None,
            dialog: None,
            toasts: Vec::new(),
            next_toast_id: 0,
            viewport: (1280.0, 800.0),
            active_streams: HashMap::new(),
            stream_senders: HashMap::new(),
            builtin_component_names: std::collections::HashSet::new(),
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
            self.register_one(comp)
                .unwrap_or_else(|e| panic!("built-in component '{}' failed to register: {}", name, e));
            self.builtin_component_names.insert(name);
        }
    }

    /// The current `iced::Theme`: the one loaded via `<link rel="theme">` if
    /// present, otherwise `Theme::Dark`. Wire it into your app with
    /// `iced::application(...).theme(|app| app.motor.theme())`.
    pub fn theme(&self) -> iced::Theme {
        self.custom_theme.clone().unwrap_or(iced::Theme::Dark)
    }

    /// Loads (or reloads) an `.gss` stylesheet from disk and re-evaluates all
    /// templates so the new classes take effect.
    ///
    /// Stylesheets are layered in load order: a class defined in a file loaded
    /// later overrides the same class from an earlier file. Loading a path that
    /// is already loaded replaces it in place (used by hot-reload).
    pub fn load_stylesheet(&mut self, path: &str) -> Result<(), String> {
        self.load_global_stylesheet_file(path)?;
        self.reevaluate_all()
    }

    /// Reads, parses and installs (or replaces in place) an external `.gss`
    /// file into the global sheet set, keyed by its own path — shared by the
    /// public [`GlacierUI::load_stylesheet`] and by `<link rel="stylesheet">`
    /// encountered while processing a template's `<link>`s. Does not
    /// re-evaluate; callers batch that themselves.
    fn load_global_stylesheet_file(&mut self, path: &str) -> Result<(), String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read stylesheet at '{}': {}", path, e))?;
        let sheet = stylesheet::StyleSheet::parse(&content)
            .map_err(|e| format!("Failed to parse stylesheet '{}': {}", path, e))?;

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
        if let Some(idx) = self.stylesheet_paths.iter().position(|p| *p == key) {
            self.stylesheets[idx] = sheet;
        } else {
            self.stylesheets.push(sheet);
            self.stylesheet_paths.push(key);
        }
    }

    /// Sets the initial active screen, clearing any navigation history.
    pub fn set_initial_screen(&mut self, name: &str) {
        self.current_screen = Some(name.to_string());
        self.history.clear();
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
    }

    /// Returns to the previous screen in the history, if any.
    pub fn navigate_back(&mut self) {
        if let Some(previous) = self.history.pop() {
            self.current_screen = Some(previous);
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
        self.toasts.push(ActiveToast { id, spec, shown_at: std::time::Instant::now() });
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
        self.toasts.retain(|t| now.duration_since(t.shown_at) < t.spec.duration);
    }

    /// Renders the current active screen, with the active dialog (if any)
    /// and any active toasts overlaid on top via [`dialogs::overlay`] and
    /// [`toasts::overlay`] — toasts on top of the dialog, since they should
    /// stay visible (and dismissible) even while a modal is up.
    pub fn render_current(&self) -> Result<iced::Element<'_, EngineMessage>, String> {
        let name = self.current_screen.as_ref()
            .ok_or_else(|| "No active screen defined; call set_initial_screen first".to_string())?;
        let screen = self.render(name)?;
        let with_dialog = match &self.dialog {
            Some(spec) => iced::widget::stack![screen, dialogs::overlay(spec, &self.theme())].into(),
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
    pub fn register_component(&mut self, name: &str, path: &str) -> Result<(), String> {
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
    pub fn register(&mut self, comp: Box<dyn component::Component>) -> Result<(), String> {
        self.register_one(comp)?;
        // Evaluate once, after the whole component tree has been registered.
        self.reevaluate_all()
    }

    /// Registers a single component and its `children()` recursively, without
    /// re-evaluating. Used by [`GlacierUI::register`].
    fn register_one(&mut self, comp: Box<dyn component::Component>) -> Result<(), String> {
        use component::Template;

        let name = comp.name().to_string();

        // (a) UI: resolve the template and feed it through the XML parse
        //     pipeline. `File` templates keep hot-reload support.
        let (markup, path) = match comp.template() {
            Template::File(path) => {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read template file at '{}': {}", path, e))?;
                let mod_time = std::fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .unwrap_or_else(|_| SystemTime::now());
                self.registered_components.insert(name.clone(), path.clone());
                self.file_mod_times.insert(name.clone(), mod_time);
                (content, Some(path))
            }
            Template::Inline(s) => (s, None),
        };

        // Parse the XML markup, with any `<script>` block stripped — its Lua
        // body is run at runtime by `LuaComponent`, not here.
        let (ast, _script) = parse_markup(path.as_deref(), &markup)
            .map_err(|e| format!("Failed to parse template for component '{}': {}", name, e))?;
        self.parsed_templates.insert(name.clone(), ast.clone());
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
            ctx.set_viewport(self.viewport);
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
                let url = self.context_data.get(target).cloned().unwrap_or_else(|| target.to_string());
                open_url(&url);
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
                        window::latest().and_then(|id| window::toggle_maximize(id))
                    }
                    "close" => window::latest().and_then(|id| window::close(id)),
                    "drag" => window::latest().and_then(|id| window::drag(id)),
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
                }
                return iced::Task::none();
            }
            EngineMessage::DialogButton(action) => {
                self.dialog = None;
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
                if new != self.viewport && self.media_set_changes(self.viewport, new) {
                    self.viewport = new;
                    let _ = self.reevaluate_all();
                } else {
                    self.viewport = new;
                }
                return iced::Task::none();
            }
            EngineMessage::UiInputChanged { action, value } => (
                action.as_str(), Some(value.as_str())
            ),
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
                return self.run_on_owner(&owner, move |comp, ctx| {
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
                return self.run_on_owner(&owner, move |comp, ctx| {
                    comp.on_stream_event(id, kind, &data, ctx);
                });
            }
            // A `after(ms, fn)` requested by a component's Lua came due: hand
            // the timer id to that component so it can call the registered
            // handler. Same shape as `LuauStream`, but one-shot.
            EngineMessage::LuauTimer { owner, id } => {
                let owner = owner.clone();
                let id = *id;
                return self.run_on_owner(&owner, move |comp, ctx| {
                    comp.resume_timer(id, ctx);
                });
            }
            // Drag-and-drop reordering of a `for-each`/`ForEach` list (see
            // `UiNode::drag_*`). `DragStart`/`DragHover` are purely internal —
            // no component ever sees them, same as `window:*` above; only
            // `DragEnd` reaches the owning `Component`, via a synthetic
            // `UiInputChanged` carrying the final order as JSON.
            EngineMessage::DragStart { list, reorder_key, on_reorder, order, key } => {
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
                self.context_data.insert(DRAG_KEY_CONTEXT.to_string(), key.clone());
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
                    reorder_context_json(&mut self.context_data, &drag.list, &drag.reorder_key, &drag.order);
                    let _ = self.reevaluate_all();
                }
                return iced::Task::none();
            }
            EngineMessage::DragEnd => {
                // Clear the "what am I dragging" marker so the highlight drops
                // once the item is released (see `DragStart`).
                self.context_data.remove(DRAG_KEY_CONTEXT);
                if let Some(drag) = self.drag.take() {
                    let value = serde_json::to_string(&drag.order).unwrap_or_else(|_| "[]".to_string());
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
            EngineMessage::UiEditorAction { binding, on_change, action } => {
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

        self.run_on_owner(&owner, move |comp, ctx| route(comp, &bare_action, ctx))
    }

    /// Borrows the component named `owner`, runs `run` against it and a fresh
    /// [`component::Context`], then applies everything the context accumulated —
    /// navigation, dialog, toasts, a re-evaluation — and turns its async
    /// requests into `iced::Task`s: each [`component::Effect`] into an
    /// `EffectOutcome` task, and each `fetch` ([`component::PendingFetch`]) into
    /// an HTTP task ([`crate::net::perform`]) whose completion comes back as
    /// [`EngineMessage::LuaResume`] to resume the suspended Lua coroutine.
    /// Shared by [`GlacierUI::route_to_owner`] and the `LuaResume` path.
    fn run_on_owner(
        &mut self,
        owner: &str,
        run: impl FnOnce(&mut dyn component::Component, &mut component::Context),
    ) -> iced::Task<EngineMessage> {
        // Disjoint per-field borrows (`components` vs `context_data`) are
        // accepted by the borrow checker when done inline like this.
        let (nav, effects, dialog, toasts, fetches, streams, stream_cmds, timers) = if let Some(comp) = self.components.get_mut(owner) {
            let mut ctx = component::Context::new(&mut self.context_data);
            ctx.set_viewport(self.viewport);
            run(comp.as_mut(), &mut ctx);
            (ctx.nav, ctx.effects, ctx.dialog, ctx.toasts, ctx.fetches, ctx.streams, ctx.stream_cmds, ctx.timers)
        } else {
            return iced::Task::none();
        };

        match nav {
            Some(component::Nav::To(s)) => self.navigate_to(&s),
            Some(component::Nav::Back) => self.navigate_back(),
            None => {}
        }

        match dialog {
            Some(component::DialogAction::Show(spec)) => self.dialog = Some(spec),
            Some(component::DialogAction::Close) => self.dialog = None,
            None => {}
        }

        for spec in toasts {
            self.show_toast(spec);
        }

        let _ = self.reevaluate_all();

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
            tasks.push(iced::Task::perform(crate::net::perform(req), move |result| {
                EngineMessage::LuauResume { owner: owner_name.clone(), id, result }
            }));
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
                move |()| EngineMessage::LuauTimer { owner: owner_name.clone(), id },
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

    /// Aggregates the [`Component::subscription`] of every registered component
    /// into a single `iced::Subscription`. Wire it into your app's
    /// `subscription(&self)` so component-owned event sources (sockets, timers)
    /// stay live; their emitted [`EngineMessage::ContextPatch`] values are just
    /// forwarded to [`GlacierUI::dispatch`].
    pub fn subscription(&self) -> iced::Subscription<EngineMessage> {
        let mut subs: Vec<_> = self.components.values().map(|c| c.subscription()).collect();
        // Global mouse-release listener: ends a reorder drag no matter where
        // the button comes up (even outside any `mouse_area`, or over a widget
        // that "captured" the event) — `dispatch` just no-ops if `self.drag` is
        // `None`, so this is always safe to keep active.
        subs.push(iced::event::listen_with(drag_end_from_event));
        subs.push(iced::event::listen_with(tab_focus_from_event));
        // Window resizes → EngineMessage::Viewport, so `@media` blocks re-resolve.
        subs.push(iced::event::listen_with(viewport_from_event));
        // Long-lived streams opened by components' Lua (`sse`/`websocket`): one
        // subscription each, keyed by `StreamKey` so the runtime keeps it alive
        // while it's in `active_streams` and drops it once removed.
        for ((owner, id), req) in &self.active_streams {
            let key = net::StreamKey {
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
    fn register_component_inner(&mut self, name: &str, path: &str) -> Result<(), String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read template file at '{}': {}", path, e))?;

        let (ast, _script) = parse_markup(Some(path), &content)
            .map_err(|e| format!("Failed to parse template for component '{}': {}", name, e))?;

        let mod_time = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| SystemTime::now());

        self.registered_components.insert(name.to_string(), path.to_string());
        self.parsed_templates.insert(name.to_string(), ast.clone());
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
    fn process_links(&mut self, component: &str, ast: &UiNode) -> Result<(), String> {
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
                    if !self.parsed_templates.contains_key(&comp_name) {
                        self.register_component_inner(&comp_name, href)?;
                    }
                }
                "data" => {
                    let key = name.clone().ok_or_else(|| format!(
                        "<link rel=\"data\" href=\"{}\"> needs an `as`/`name` attribute for the context key",
                        href
                    ))?;
                    self.load_data_file(&key, href)?;
                }
                "theme" => self.load_theme_file(href)?,
                other => {
                    return Err(format!(
                        "Unsupported <link rel=\"{}\"> (href=\"{}\"); expected stylesheet, import, component, data or theme",
                        other, href
                    ));
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

        if scoped_css.is_empty() {
            self.component_stylesheets.remove(component);
        } else {
            let sheets = scoped_css.iter()
                .map(|css| stylesheet::StyleSheet::parse(css)
                    .map_err(|e| format!("Failed to parse inline scoped <style> of '{}': {}", component, e)))
                .collect::<Result<Vec<_>, _>>()?;
            self.component_stylesheets.insert(component.to_string(), sheets);
        }

        for (idx, css) in global_css.iter().enumerate() {
            let sheet = stylesheet::StyleSheet::parse(css)
                .map_err(|e| format!("Failed to parse inline <style> of '{}': {}", component, e))?;
            self.install_global_stylesheet(inline_style_key(component, idx), sheet);
        }

        Ok(())
    }

    /// Loads a JSON `data` file and merges it into the context under `key`:
    /// an object's top-level fields become `key.field`; an array or scalar is
    /// stored as `key`. Tracks the source for hot-reload.
    fn load_data_file(&mut self, key: &str, path: &str) -> Result<(), String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read data file '{}': {}", path, e))?;
        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("Data file '{}' is not valid JSON: {}", path, e))?;

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
    fn load_theme_file(&mut self, path: &str) -> Result<(), String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read theme file '{}': {}", path, e))?;
        let theme = parse_theme(&content)
            .map_err(|e| format!("Failed to parse theme '{}': {}", path, e))?;

        let mod_time = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| SystemTime::now());
        self.file_mod_times.insert(theme_key(path), mod_time);
        self.custom_theme = Some(theme);
        self.theme_path = Some(path.to_string());
        Ok(())
    }

    /// Walks a parsed tree and registers every `<import>`ed component not yet loaded.
    fn load_imports(&mut self, node: &UiNode) -> Result<(), String> {
        if let NodeType::Import { name, from } = &node.kind {
            // Load if the name is free, or if it currently holds a builtin the
            // app is deliberately shadowing (an explicit `<import>` wins over a
            // lib builtin). Once overridden, the name is a normal component and
            // later imports of it are skipped as before.
            let is_builtin = self.builtin_component_names.contains(name);
            if !self.parsed_templates.contains_key(name) || is_builtin {
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
    pub fn reevaluate_all(&mut self) -> Result<(), String> {
        // Qualquer sheet (global ou de escopo) com seletor de tag liga a
        // resolução de estilo para nós sem class/id — calculado uma vez aqui
        // para não pagar por nó no caso comum (nenhum seletor de tag).
        let has_tag_rules = self.stylesheets.iter().any(|s| s.has_tag_rules())
            || self.component_stylesheets.values().flatten().any(|s| s.has_tag_rules());
        let styles = StyleContext {
            global: &self.stylesheets,
            by_component: &self.component_stylesheets,
            viewport: Some(self.viewport),
            has_tag_rules,
        };
        let mut evals = HashMap::new();
        for (name, template_ast) in &self.parsed_templates {
            // The template's own name is the style scope, so its `<link>`ed
            // sheets apply to its subtree.
            let evaluated_ast = evaluate_node(
                template_ast,
                &self.context_data,
                &self.parsed_templates,
                &styles,
                Some(name),
            )?;
            evals.insert(name.clone(), evaluated_ast);
        }
        self.evaluated_templates = evals;
        self.sync_editors();
        Ok(())
    }

    /// `true` se mover o viewport de `old` para `new` ativa ou desativa algum
    /// bloco `@media` (global ou com escopo) — usado por `dispatch` para só
    /// re-avaliar quando o resultado das media queries realmente muda.
    fn media_set_changes(&self, old: (f32, f32), new: (f32, f32)) -> bool {
        let sheets = self
            .stylesheets
            .iter()
            .chain(self.component_stylesheets.values().flatten());
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
                self.editors.insert(b.clone(), iced::widget::text_editor::Content::with_text(&ctx_val));
                self.editor_synced.insert(b, ctx_val);
            }
        }
    }

    /// Recursively evaluates the component and translates it into an Iced Element
    pub fn render<'a>(&'a self, component_name: &str) -> Result<iced::Element<'a, EngineMessage>, String> {
        let evaluated_ast = self.evaluated_templates.get(component_name)
            .ok_or_else(|| format!("Component '{}' is not evaluated or registered", component_name))?;

        // Render the evaluated AST to Iced Widgets
        Ok(render_node(evaluated_ast, &self.context_data, &self.editors))
    }

    /// Checks registered XML files for changes and re-parses them if modified.
    /// Returns the list of component names that were reloaded.
    pub fn check_reload(&mut self) -> Vec<String> {
        let mut reloaded = Vec::new();
        let mut updates = Vec::new();

        for (name, path) in &self.registered_components {
            if let Ok(metadata) = std::fs::metadata(path) {
                if let Ok(modified) = metadata.modified() {
                    let last_modified = self.file_mod_times.get(name);
                    if last_modified.map_or(true, |&last| modified > last) {
                        // File changed, reload it (XML).
                        if let Ok(content) = std::fs::read_to_string(path) {
                            if let Ok((new_ast, _script)) = parse_markup(Some(path.as_str()), &content) {
                                updates.push((name.clone(), new_ast, modified));
                                reloaded.push(name.clone());
                            }
                        }
                    }
                }
            }
        }

        // Detect changed `.gss` files the same way. Only global sheets carry a
        // real path to watch — inline `<style>` blocks (global or scoped) are
        // rebuilt when their declaring template reloads, in the loop below.
        // (An inline global block's synthetic key simply misses `fs::metadata`
        // and is skipped here, harmlessly.)
        let all_paths = self.stylesheet_paths.clone();
        let mut sheet_updates = Vec::new();
        for path in &all_paths {
            if let Ok(modified) = std::fs::metadata(path).and_then(|m| m.modified()) {
                let last_modified = self.file_mod_times.get(&stylesheet_key(path));
                if last_modified.map_or(true, |&last| modified > last) {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        match stylesheet::StyleSheet::parse(&content) {
                            Ok(sheet) => {
                                sheet_updates.push((path.clone(), sheet, modified));
                                reloaded.push(path.clone());
                            }
                            Err(e) => eprintln!("Stylesheet '{}' has an error, keeping the previous version: {}", path, e),
                        }
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
            self.parsed_templates.insert(name.clone(), new_ast);
            self.file_mod_times.insert(name, modified);
        }

        // Apply stylesheet changes in place, preserving load order/priority.
        for (path, sheet, modified) in sheet_updates {
            if let Some(idx) = self.stylesheet_paths.iter().position(|p| *p == path) {
                self.stylesheets[idx] = sheet;
            }
            self.file_mod_times.insert(stylesheet_key(&path), modified);
        }

        // Reload a changed `<link rel="data">` JSON file (re-merged into context).
        for (key, path) in self.data_sources.clone() {
            if let Ok(modified) = std::fs::metadata(&path).and_then(|m| m.modified()) {
                let changed = self.file_mod_times.get(&data_key(&path)).map_or(true, |&last| modified > last);
                if changed {
                    match self.load_data_file(&key, &path) {
                        Ok(()) => {
                            reloaded.push(path.clone());
                            dirty = true;
                        }
                        Err(e) => eprintln!("Data file '{}' has an error, keeping the previous version: {}", path, e),
                    }
                }
            }
        }

        // Reload a changed `<link rel="theme">` palette file.
        if let Some(path) = self.theme_path.clone() {
            if let Ok(modified) = std::fs::metadata(&path).and_then(|m| m.modified()) {
                let changed = self.file_mod_times.get(&theme_key(&path)).map_or(true, |&last| modified > last);
                if changed {
                    match self.load_theme_file(&path) {
                        Ok(()) => {
                            reloaded.push(path.clone());
                            dirty = true;
                        }
                        Err(e) => eprintln!("Theme '{}' has an error, keeping the previous version: {}", path, e),
                    }
                }
            }
        }

        if dirty {
            // Re-evaluate all templates against the new markup/styles/data.
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
/// `<script>` body (if any). Templates are XML; `path` is kept in the signature
/// (currently unused) so call sites can pass the source path for future
/// diagnostics.
fn parse_markup(_path: Option<&str>, content: &str) -> Result<(UiNode, Option<String>), String> {
    let (markup, script) = eval::strip_script(content);
    let markup = eval::normalize_bare_directives(&markup);
    Ok((UiNode::parse_xml(&markup)?, script))
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
    if let NodeType::TextArea { value_var, .. } = &node.kind {
        if !value_var.is_empty() && !out.contains(value_var) {
            out.push(value_var.clone());
        }
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
fn collect_inline_styles(node: &UiNode, scoped_out: &mut Vec<String>, global_out: &mut Vec<String>) {
    if let NodeType::Style { css, scoped } = &node.kind {
        if !css.trim().is_empty() {
            if *scoped {
                scoped_out.push(css.clone());
            } else {
                global_out.push(css.clone());
            }
        }
    }
    for child in &node.children {
        collect_inline_styles(child, scoped_out, global_out);
    }
}

/// Collects every `<link>` in a parsed tree as `(rel, href, name)`, in
/// document order. Links with an empty `href` are skipped.
fn collect_links(node: &UiNode, out: &mut Vec<(String, String, Option<String>)>) {
    if let NodeType::Link { rel, href, name } = &node.kind {
        if !href.is_empty() {
            out.push((rel.clone(), href.clone(), name.clone()));
        }
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
        component::StreamKind::Sse => {
            net::sse(key.url.clone(), key.headers.clone()).left_stream()
        }
        component::StreamKind::Ws => {
            net::websocket(key.url.clone(), key.headers.clone()).right_stream()
        }
    };
    events.map(move |event| EngineMessage::LuauStream { owner: owner.clone(), id, event })
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
    use iced::keyboard::{key::Named, Event as Kbd, Key};
    match event {
        iced::Event::Keyboard(Kbd::KeyPressed { key: Key::Named(Named::Tab), modifiers, .. }) => {
            Some(if modifiers.shift() { EngineMessage::FocusPrev } else { EngineMessage::FocusNext })
        }
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
        iced::Event::Window(iced::window::Event::Resized(size)) => {
            Some(EngineMessage::Viewport { width: size.width, height: size.height })
        }
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
    let Some(json_str) = context.get(list) else { return };
    let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(json_str) else { return };

    let mut by_key: HashMap<String, serde_json::Value> = HashMap::new();
    let mut leftovers: Vec<serde_json::Value> = Vec::new();
    for item in arr {
        match item.get(reorder_key).and_then(|v| v.as_str()).map(String::from) {
            Some(k) => { by_key.insert(k, item); }
            None => leftovers.push(item),
        }
    }
    let mut reordered: Vec<serde_json::Value> = order
        .iter()
        .filter_map(|k| by_key.remove(k))
        .collect();
    reordered.extend(by_key.into_values());
    reordered.extend(leftovers);

    if let Ok(new_json) = serde_json::to_string(&serde_json::Value::Array(reordered)) {
        context.insert(list.to_string(), new_json);
    }
}

/// Merges a parsed JSON `value` into `context` under `key`:
/// - an object's top-level fields become `key.field` entries;
/// - an array or scalar is stored as the single entry `key`.
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
fn parse_theme(content: &str) -> Result<iced::Theme, String> {
    let value: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| format!("not valid JSON: {}", e))?;
    let obj = value.as_object()
        .ok_or_else(|| "theme must be a JSON object of colors".to_string())?;

    let name = obj.get("name").and_then(|n| n.as_str()).unwrap_or("custom").to_string();
    let color = |field: &str| -> Result<iced::Color, String> {
        let hex = obj.get(field)
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("missing color '{}'", field))?;
        widget::parse_hex_color(hex)
            .ok_or_else(|| format!("invalid hex color '{}' for '{}'", hex, field))
    };

    let palette = iced::theme::Palette {
        background: color("background")?,
        text: color("text")?,
        primary: color("primary")?,
        success: color("success")?,
        // `warning` was added to the palette in iced 0.14; keep it optional in
        // the theme JSON and fall back to a sensible amber when absent.
        warning: color("warning")
            .unwrap_or_else(|_| widget::parse_hex_color("#D29922").unwrap()),
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
        assert_eq!(only_toast.toast.as_ref().map(|t| t.kind), Some(ToastKind::Success));

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
