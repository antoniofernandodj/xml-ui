pub mod parser;
pub mod eval;
pub mod widget;
pub mod component;

pub use parser::{UiNode, NodeType};
pub use eval::{evaluate_node, process_template, strip_script};
pub use widget::{render_node, EngineMessage};
pub use component::{Component, Context, ContextVar, Nav, Template};

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
        let mut evals = HashMap::new();
        for (name, template_ast) in &self.parsed_templates {
            let evaluated_ast = evaluate_node(template_ast, &self.context_data, &self.parsed_templates)?;
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

        if !updates.is_empty() {
            // Apply changes
            for (name, new_ast, modified) in updates {
                // Pick up any newly-added `<import>` declarations.
                let _ = self.load_imports(&new_ast);
                self.parsed_templates.insert(name.clone(), new_ast);
                self.file_mod_times.insert(name, modified);
            }
            // Re-evaluate all templates
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
