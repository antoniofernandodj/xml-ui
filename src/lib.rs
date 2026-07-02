pub mod parser;
pub mod kdl_parser;
pub mod eval;
pub mod widget;
pub mod component;
pub mod stylesheet;
pub mod forms;
pub mod dialogs;
pub mod toasts;

pub use parser::{UiNode, NodeType};
pub use kdl_parser::{parse_kdl, register_bare_flags};
pub use eval::{evaluate_node, process_template, strip_script, normalize_bare_directives, StyleContext};
pub use widget::{render_node, EngineMessage};
pub use component::{Component, Context, ContextVar, DialogAction, Effect, Nav, Template};
pub use stylesheet::{StyleSheet, StyleRule};
pub use forms::{Form, FormBuilder, FormControl, Validator};
pub use dialogs::{ButtonRole, DialogButton, DialogIcon, DialogSpec};
pub use toasts::{ToastKind, ToastSpec};

/// Derives `impl Component` from a struct plus the `<script>` block of an XML
/// template. See the `contador_macro` example.
pub use glacier_ui_macros::component;

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
    /// Per-component (scoped) stylesheets declared via `<link rel="stylesheet">`
    /// or an inline `<style>` block, keyed by component name. Applied on top of
    /// the global sheets, but only inside that component's subtree, in document
    /// order.
    pub component_stylesheets: HashMap<String, Vec<stylesheet::StyleSheet>>,
    /// Source path of each component's scoped sheets (parallel to the vecs
    /// above), kept for hot-reload. An inline `<style>` block has no file, so its
    /// slot is `None` — it is rebuilt from the markup when the template reloads.
    pub component_stylesheet_paths: HashMap<String, Vec<Option<String>>>,
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
        Self {
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
            component_stylesheet_paths: HashMap::new(),
            custom_theme: None,
            theme_path: None,
            data_sources: Vec::new(),
            editors: HashMap::new(),
            editor_synced: HashMap::new(),
            drag: None,
            dialog: None,
            toasts: Vec::new(),
            next_toast_id: 0,
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
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read stylesheet at '{}': {}", path, e))?;
        let sheet = stylesheet::StyleSheet::parse(&content)
            .map_err(|e| format!("Failed to parse stylesheet '{}': {}", path, e))?;

        let mod_time = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| SystemTime::now());

        if let Some(idx) = self.stylesheet_paths.iter().position(|p| p == path) {
            self.stylesheets[idx] = sheet;
        } else {
            self.stylesheets.push(sheet);
            self.stylesheet_paths.push(path.to_string());
        }
        self.file_mod_times.insert(stylesheet_key(path), mod_time);

        self.reevaluate_all()
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
    fn register_one(&mut self, mut comp: Box<dyn component::Component>) -> Result<(), String> {
        use component::Template;

        let name = comp.name().to_string();

        // (a) UI: resolve the template and feed it through the parse pipeline,
        //     which picks XML or KDL by the file extension. `File` templates
        //     keep hot-reload support.
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

        // Parse via XML (with `<script>` stripped, behavior compiled in by
        // `#[component]`) or KDL, depending on the extension.
        let (ast, _script) = parse_markup(path.as_deref(), &markup)
            .map_err(|e| format!("Failed to parse template for component '{}': {}", name, e))?;
        self.parsed_templates.insert(name.clone(), ast.clone());
        self.load_imports(&ast)?;
        self.process_links(&name, &ast)?;

        // (b) Behavior: let the component seed its initial state.
        {
            let mut ctx = component::Context { data: &mut self.context_data, nav: None, effects: Vec::new(), dialog: None, toasts: Vec::new() };
            comp.init(&mut ctx);
        }

        // (c) Children: collect before moving `comp` into the map, then register
        //     each recursively so their templates/behavior are available too.
        let children = comp.children();
        self.components.insert(name, comp);
        for child in children {
            self.register_one(child)?;
        }
        Ok(())
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
            // Built-in window controls: drive the host window without any
            // component code, so a borderless app can wire its custom titlebar
            // straight from markup — `onClick="window:close"` for the buttons,
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
                (prefix.to_string(), rest)
            }
            Some((_, rest)) => match &self.current_screen {
                Some(screen) => (screen.clone(), rest),
                None => return iced::Task::none(),
            },
            None => match &self.current_screen {
                Some(screen) => (screen.clone(), action),
                None => return iced::Task::none(),
            },
        };

        // Disjoint per-field borrows (`components` vs `context_data`) are
        // accepted by the borrow checker when done inline like this.
        let (nav, effects, dialog, toasts) = if let Some(comp) = self.components.get_mut(&owner) {
            let mut ctx = component::Context { data: &mut self.context_data, nav: None, effects: Vec::new(), dialog: None, toasts: Vec::new() };
            route(comp.as_mut(), bare_action, &mut ctx);
            (ctx.nav, ctx.effects, ctx.dialog, ctx.toasts)
        } else {
            (None, Vec::new(), None, Vec::new())
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

        // Turn each requested effect into an iced Task whose completion feeds a
        // ContextPatch back through dispatch.
        let tasks = effects.into_iter().map(|effect| match effect {
            component::Effect::Perform(future) => {
                iced::Task::perform(future, EngineMessage::ContextPatch)
            }
        });
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

        // Recursively load components declared with `<import>`.
        self.load_imports(&ast)?;
        // Process this component's `<link>` declarations.
        self.process_links(name, &ast)?;

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

        // The rest of the `<link>` kinds are global side effects applied
        // immediately. Scoped stylesheets (linked or inline `<style>`) are
        // gathered separately below, preserving document order.
        for (rel, href, name) in &links {
            match rel.as_str() {
                // Scoped styles are handled by `collect_scoped_styles` below.
                "stylesheet" => {}
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

        // Apply (rebuild or clear) this component's scoped stylesheets. Both
        // linked `.gss` files and inline `<style>` blocks contribute, in the
        // order they appear in the markup, so later rules win on a tie.
        let mut scoped = Vec::new();
        collect_scoped_styles(ast, &mut scoped);
        if scoped.is_empty() {
            self.component_stylesheets.remove(component);
            self.component_stylesheet_paths.remove(component);
        } else {
            let mut sheets = Vec::with_capacity(scoped.len());
            let mut paths: Vec<Option<String>> = Vec::with_capacity(scoped.len());
            for source in &scoped {
                match source {
                    ScopedStyle::Linked(href) => {
                        let content = std::fs::read_to_string(href)
                            .map_err(|e| format!("Failed to read stylesheet '{}' linked by '{}': {}", href, component, e))?;
                        let sheet = stylesheet::StyleSheet::parse(&content)
                            .map_err(|e| format!("Failed to parse stylesheet '{}' linked by '{}': {}", href, component, e))?;
                        let mod_time = std::fs::metadata(href)
                            .and_then(|m| m.modified())
                            .unwrap_or_else(|_| SystemTime::now());
                        self.file_mod_times.insert(stylesheet_key(href), mod_time);
                        sheets.push(sheet);
                        paths.push(Some(href.clone()));
                    }
                    ScopedStyle::Inline(css) => {
                        let sheet = stylesheet::StyleSheet::parse(css)
                            .map_err(|e| format!("Failed to parse inline <style> of '{}': {}", component, e))?;
                        sheets.push(sheet);
                        paths.push(None);
                    }
                }
            }
            self.component_stylesheets.insert(component.to_string(), sheets);
            self.component_stylesheet_paths.insert(component.to_string(), paths);
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
            if !self.parsed_templates.contains_key(name) {
                let (name, from) = (name.clone(), from.clone());
                self.register_component_inner(&name, &from)?;
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
        let styles = StyleContext {
            global: &self.stylesheets,
            by_component: &self.component_stylesheets,
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
                        // File changed, reload it (XML or KDL by extension).
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

        // Detect changed `.gss` stylesheets the same way. Both global sheets and
        // per-component (`<link>`-scoped) sheets are watched; a path used in more
        // than one place is only re-parsed once.
        let mut all_paths: Vec<String> = self.stylesheet_paths.clone();
        for paths in self.component_stylesheet_paths.values() {
            for p in paths.iter().flatten() {
                if !all_paths.contains(p) {
                    all_paths.push(p.clone());
                }
            }
        }
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

        // Apply stylesheet changes in place (preserving load order/priority),
        // updating every global and scoped slot that references the path.
        for (path, sheet, modified) in sheet_updates {
            if let Some(idx) = self.stylesheet_paths.iter().position(|p| *p == path) {
                self.stylesheets[idx] = sheet.clone();
            }
            // Collect (component, index) targets first to avoid borrowing
            // `component_stylesheet_paths` while mutating `component_stylesheets`.
            let targets: Vec<(String, usize)> = self.component_stylesheet_paths.iter()
                .flat_map(|(comp, paths)| {
                    paths.iter().enumerate()
                        .filter(|(_, p)| p.as_deref() == Some(path.as_str()))
                        .map(move |(i, _)| (comp.clone(), i))
                })
                .collect();
            for (comp, i) in targets {
                if let Some(slot) = self.component_stylesheets.get_mut(&comp).and_then(|v| v.get_mut(i)) {
                    *slot = sheet.clone();
                }
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

/// Parses a template's source into a [`UiNode`], picking the parser by the
/// file extension: `.kdl` uses the KDL parser, anything else (including unknown
/// extensions and inline templates with no path) falls back to XML.
///
/// `path` is `None` for inline templates. The returned tuple carries the
/// `<script>` body for XML templates (KDL strips its `script` block internally
/// and never surfaces one here).
fn parse_markup(path: Option<&str>, content: &str) -> Result<(UiNode, Option<String>), String> {
    if path.is_some_and(|p| p.to_ascii_lowercase().ends_with(".kdl")) {
        Ok((parse_kdl(content)?, None))
    } else {
        let (markup, script) = eval::strip_script(content);
        let markup = eval::normalize_bare_directives(&markup);
        Ok((UiNode::parse_xml(&markup)?, script))
    }
}

/// Namespaced keys under which a resource's modification time is stored in
/// `file_mod_times`, so they never collide with a component of the same name
/// (or with each other across resource kinds).
fn stylesheet_key(path: &str) -> String {
    format!("gss::{}", path)
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

/// The file stem of a path (`templates/perfil_card.xml` -> `perfil_card`),
/// used as the default component name for `<link rel="import">`.
fn file_stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

/// A component-scoped stylesheet source, in document order: either an external
/// `.gss` file linked via `<link rel="stylesheet">` / `style "path"`, or the
/// inline `.gss` body of a `<style>` block.
enum ScopedStyle {
    Linked(String),
    Inline(String),
}

/// Collects a component's scoped stylesheet sources (linked `.gss` files and
/// inline `<style>` blocks) in document order, so they can be layered with the
/// later ones winning. Empty hrefs / blank inline bodies are skipped.
fn collect_scoped_styles(node: &UiNode, out: &mut Vec<ScopedStyle>) {
    match &node.kind {
        NodeType::Link { rel, href, .. } if rel == "stylesheet" && !href.is_empty() => {
            out.push(ScopedStyle::Linked(href.clone()));
        }
        NodeType::Style { css } if !css.trim().is_empty() => {
            out.push(ScopedStyle::Inline(css.clone()));
        }
        _ => {}
    }
    for child in &node.children {
        collect_scoped_styles(child, out);
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
