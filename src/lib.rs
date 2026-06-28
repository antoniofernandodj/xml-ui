pub mod parser;
pub mod eval;
pub mod widget;
pub mod component;
pub mod stylesheet;

pub use parser::{UiNode, NodeType};
pub use eval::{evaluate_node, process_template, strip_script, StyleContext};
pub use widget::{render_node, EngineMessage};
pub use component::{Component, Context, ContextVar, Nav, Template};
pub use stylesheet::{StyleSheet, StyleRule};

/// Derives `impl Component` from a struct plus the `<script>` block of an XML
/// template. See the `contador_macro` example.
pub use xml_ui_macros::component;

use std::collections::HashMap;
use std::time::{SystemTime, Duration};

/// The XML-to-UI rendering engine
pub struct UiEngine {
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
    /// Globally-loaded `.iss` stylesheets, in ascending priority order (a class
    /// defined in a later sheet overrides the same class in an earlier one).
    pub stylesheets: Vec<stylesheet::StyleSheet>,
    /// Paths of loaded global `.iss` files (parallel to `stylesheets`), kept for
    /// hot-reload along with their last-seen modification times.
    pub stylesheet_paths: Vec<String>,
    /// Per-component (scoped) stylesheets declared via `<link rel="stylesheet">`,
    /// keyed by component name. Applied on top of the global sheets, but only
    /// inside that component's subtree.
    pub component_stylesheets: HashMap<String, Vec<stylesheet::StyleSheet>>,
    /// Paths of each component's scoped sheets (parallel to the vecs above),
    /// kept for hot-reload.
    pub component_stylesheet_paths: HashMap<String, Vec<String>>,
    /// The custom `iced::Theme` loaded via `<link rel="theme">`, if any.
    /// Apps read it through [`UiEngine::theme`].
    pub custom_theme: Option<iced::Theme>,
    /// Path of the loaded theme file, kept for hot-reload.
    theme_path: Option<String>,
    /// Data files loaded via `<link rel="data">`, as `(context key, path)`,
    /// kept for hot-reload.
    data_sources: Vec<(String, String)>,
}

impl UiEngine {
    /// Creates a new, empty UiEngine instance
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
        }
    }

    /// The current `iced::Theme`: the one loaded via `<link rel="theme">` if
    /// present, otherwise `Theme::Dark`. Wire it into your app with
    /// `iced::application(...).theme(|app| app.motor.theme())`.
    pub fn theme(&self) -> iced::Theme {
        self.custom_theme.clone().unwrap_or(iced::Theme::Dark)
    }

    /// Loads (or reloads) an `.iss` stylesheet from disk and re-evaluates all
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

    /// Renders the current active screen.
    pub fn render_current(&self) -> Result<iced::Element<'_, EngineMessage>, String> {
        let name = self.current_screen.as_ref()
            .ok_or_else(|| "No active screen defined; call set_initial_screen first".to_string())?;
        self.render(name)
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
    /// component so that [`UiEngine::dispatch`] can later route actions to it.
    pub fn register(&mut self, comp: Box<dyn component::Component>) -> Result<(), String> {
        self.register_one(comp)?;
        // Evaluate once, after the whole component tree has been registered.
        self.reevaluate_all()
    }

    /// Registers a single component and its `children()` recursively, without
    /// re-evaluating. Used by [`UiEngine::register`].
    fn register_one(&mut self, mut comp: Box<dyn component::Component>) -> Result<(), String> {
        use component::Template;

        let name = comp.name().to_string();

        // (a) UI: resolve the template and feed it through the existing parse
        //     pipeline. `File` templates keep hot-reload support.
        let xml = match comp.template() {
            Template::File(path) => {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read XML file at '{}': {}", path, e))?;
                let mod_time = std::fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .unwrap_or_else(|_| SystemTime::now());
                self.registered_components.insert(name.clone(), path);
                self.file_mod_times.insert(name.clone(), mod_time);
                content
            }
            Template::Inline(s) => s,
        };

        // Strip any `<script>` block (behavior is compiled in by `#[component]`).
        let (markup, _script) = eval::strip_script(&xml);
        let ast = UiNode::parse_xml(&markup)
            .map_err(|e| format!("Failed to parse XML for component '{}': {}", name, e))?;
        self.parsed_templates.insert(name.clone(), ast.clone());
        self.load_imports(&ast)?;
        self.process_links(&name, &ast)?;

        // (b) Behavior: let the component seed its initial state.
        {
            let mut ctx = component::Context { data: &mut self.context_data, nav: None };
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
    /// Apps that use [`UiEngine::register`] just forward every message here from
    /// their `update()` instead of matching on actions themselves.
    pub fn dispatch(&mut self, msg: &EngineMessage) -> Result<(), String> {
        let (action, value) = match msg {
            EngineMessage::XmlClick(a) => (a.as_str(), None),
            EngineMessage::XmlInputChanged { action, value } => (action.as_str(), Some(value.as_str())),
            EngineMessage::Navigate(s) => {
                self.navigate_to(s);
                return self.reevaluate_all();
            }
            EngineMessage::NavigateBack => {
                self.navigate_back();
                return self.reevaluate_all();
            }
            EngineMessage::FileChanged(_) => {
                self.check_reload();
                return Ok(());
            }
        };

        // Resolve which component owns the action. An action namespaced as
        // `Child::act` (produced when a `<Component>` subtree is inlined) routes
        // to `Child` when it has registered behavior; otherwise — and for plain,
        // un-namespaced actions — it falls back to the active screen.
        let (owner, action) = match action.split_once("::") {
            Some((prefix, rest)) if self.components.contains_key(prefix) => {
                (prefix.to_string(), rest)
            }
            Some((_, rest)) => match &self.current_screen {
                Some(screen) => (screen.clone(), rest),
                None => return Ok(()),
            },
            None => match &self.current_screen {
                Some(screen) => (screen.clone(), action),
                None => return Ok(()),
            },
        };

        // Disjoint per-field borrows (`components` vs `context_data`) are
        // accepted by the borrow checker when done inline like this.
        let nav = if let Some(comp) = self.components.get_mut(&owner) {
            let mut ctx = component::Context { data: &mut self.context_data, nav: None };
            comp.update(action, value, &mut ctx);
            ctx.nav
        } else {
            None
        };

        match nav {
            Some(component::Nav::To(s)) => self.navigate_to(&s),
            Some(component::Nav::Back) => self.navigate_back(),
            None => {}
        }

        self.reevaluate_all()
    }

    /// Parses and stores a component plus its imports, without re-evaluating.
    fn register_component_inner(&mut self, name: &str, path: &str) -> Result<(), String> {
        let xml_content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read XML file at '{}': {}", path, e))?;

        let (markup, _script) = eval::strip_script(&xml_content);
        let ast = UiNode::parse_xml(&markup)
            .map_err(|e| format!("Failed to parse XML for component '{}': {}", name, e))?;

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

        // Stylesheet links are scoped to `component`; the rest are global
        // side effects applied immediately.
        let mut sheet_hrefs = Vec::new();
        for (rel, href, name) in &links {
            match rel.as_str() {
                "stylesheet" => sheet_hrefs.push(href.clone()),
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

        // Apply (rebuild or clear) this component's scoped stylesheets.
        if sheet_hrefs.is_empty() {
            self.component_stylesheets.remove(component);
            self.component_stylesheet_paths.remove(component);
        } else {
            let mut sheets = Vec::with_capacity(sheet_hrefs.len());
            for href in &sheet_hrefs {
                let content = std::fs::read_to_string(href)
                    .map_err(|e| format!("Failed to read stylesheet '{}' linked by '{}': {}", href, component, e))?;
                let sheet = stylesheet::StyleSheet::parse(&content)
                    .map_err(|e| format!("Failed to parse stylesheet '{}' linked by '{}': {}", href, component, e))?;
                let mod_time = std::fs::metadata(href)
                    .and_then(|m| m.modified())
                    .unwrap_or_else(|_| SystemTime::now());
                self.file_mod_times.insert(stylesheet_key(href), mod_time);
                sheets.push(sheet);
            }
            self.component_stylesheets.insert(component.to_string(), sheets);
            self.component_stylesheet_paths.insert(component.to_string(), sheet_hrefs);
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
        Ok(())
    }

    /// Recursively evaluates the component and translates it into an Iced Element
    pub fn render<'a>(&'a self, component_name: &str) -> Result<iced::Element<'a, EngineMessage>, String> {
        let evaluated_ast = self.evaluated_templates.get(component_name)
            .ok_or_else(|| format!("Component '{}' is not evaluated or registered", component_name))?;

        // Render the evaluated AST to Iced Widgets
        Ok(render_node(evaluated_ast, &self.context_data))
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
                        // File changed, reload it
                        if let Ok(xml_content) = std::fs::read_to_string(path) {
                            let (markup, _script) = eval::strip_script(&xml_content);
                            if let Ok(new_ast) = UiNode::parse_xml(&markup) {
                                updates.push((name.clone(), new_ast, modified));
                                reloaded.push(name.clone());
                            }
                        }
                    }
                }
            }
        }

        // Detect changed `.iss` stylesheets the same way. Both global sheets and
        // per-component (`<link>`-scoped) sheets are watched; a path used in more
        // than one place is only re-parsed once.
        let mut all_paths: Vec<String> = self.stylesheet_paths.clone();
        for paths in self.component_stylesheet_paths.values() {
            for p in paths {
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
                        .filter(|(_, p)| **p == path)
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
}

/// Namespaced keys under which a resource's modification time is stored in
/// `file_mod_times`, so they never collide with a component of the same name
/// (or with each other across resource kinds).
fn stylesheet_key(path: &str) -> String {
    format!("iss::{}", path)
}
fn data_key(path: &str) -> String {
    format!("data::{}", path)
}
fn theme_key(path: &str) -> String {
    format!("theme::{}", path)
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
        danger: color("danger")?,
    };
    Ok(iced::Theme::custom(name, palette))
}
