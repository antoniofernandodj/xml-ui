use xml_ui::{UiEngine, UiNode, NodeType};

#[test]
fn test_parser_basic() {
    let xml = r##"
    <Container padding="15" width="200" background="#FFFFFF">
        <Column spacing="10">
            <Text content="Hello World" size="20" bold="true" />
            <Button text="Click Me" onClick="btn_click" />
        </Column>
    </Container>
    "##;

    let ast = UiNode::parse_xml(xml).unwrap();
    
    assert_eq!(ast.kind, NodeType::Container);
    assert_eq!(ast.padding.as_deref(), Some("15"));
    assert_eq!(ast.width.as_deref(), Some("200"));
    assert_eq!(ast.background.as_deref(), Some("#FFFFFF"));
    
    assert_eq!(ast.children.len(), 1);
    let column = &ast.children[0];
    assert_eq!(column.kind, NodeType::Column);
    assert_eq!(column.spacing, Some(10.0));
    
    assert_eq!(column.children.len(), 2);
    
    let text = &column.children[0];
    if let NodeType::Text { content, size, bold, .. } = &text.kind {
        assert_eq!(content, "Hello World");
        assert_eq!(*size, Some(20.0));
        assert!(bold);
    } else {
        panic!("First child of Column should be Text");
    }

    let button = &column.children[1];
    if let NodeType::Button { text, on_click, .. } = &button.kind {
        assert_eq!(text, "Click Me");
        assert_eq!(on_click.as_deref(), Some("btn_click"));
    } else {
        panic!("Second child of Column should be Button");
    }
}

#[test]
fn test_interpolation() {
    let mut motor = UiEngine::new();
    
    let temp_xml_path = "templates/test_temp.xml";
    std::fs::create_dir_all("templates").ok();
    std::fs::write(
        temp_xml_path,
        r##"<Text content="Welcome, {user_name}! Role: {user_role}" />"##
    ).unwrap();

    motor.register_component("test_comp", temp_xml_path).unwrap();
    
    motor.define_data("user_name", "Bob");
    motor.define_data("user_role", "Admin");

    let evaluated = motor.evaluated_templates.get("test_comp").unwrap();
    if let NodeType::Text { content, .. } = &evaluated.kind {
        assert_eq!(content, "Welcome, Bob! Role: Admin");
    } else {
        panic!("Root node should be evaluated Text");
    }

    std::fs::remove_file(temp_xml_path).ok();
}

#[test]
fn test_includes() {
    let mut motor = UiEngine::new();
    
    std::fs::create_dir_all("templates").ok();
    
    let main_path = "templates/test_main.xml";
    let card_path = "templates/test_card.xml";

    std::fs::write(
        card_path,
        r##"<Container background="#222"><Text content="User: {name}" /></Container>"##
    ).unwrap();

    std::fs::write(
        main_path,
        r##"
        <Column>
            <Include src="test_card" name="Alice" />
            <Include src="test_card" name="Charlie" />
        </Column>
        "##
    ).unwrap();

    motor.register_component("test_card", card_path).unwrap();
    motor.register_component("test_main", main_path).unwrap();

    let evaluated = motor.evaluated_templates.get("test_main").unwrap();
    assert_eq!(evaluated.kind, NodeType::Column);
    assert_eq!(evaluated.children.len(), 2);

    let first_child = &evaluated.children[0];
    assert_eq!(first_child.kind, NodeType::Container);
    if let NodeType::Text { content, .. } = &first_child.children[0].kind {
        assert_eq!(content, "User: Alice");
    } else {
        panic!("Included first child should contain text 'User: Alice'");
    }

    let second_child = &evaluated.children[1];
    if let NodeType::Text { content, .. } = &second_child.children[0].kind {
        assert_eq!(content, "User: Charlie");
    } else {
        panic!("Included second child should contain text 'User: Charlie'");
    }

    std::fs::remove_file(main_path).ok();
    std::fs::remove_file(card_path).ok();
}

#[test]
fn test_if_else() {
    let mut motor = UiEngine::new();

    std::fs::create_dir_all("templates").ok();
    let path = "templates/test_if.xml";
    std::fs::write(
        path,
        r##"
        <Column>
            <if cond="{logado}">
                <Text content="Olá, {usuario}" />
            </if>
            <else>
                <Text content="Entre, por favor" />
            </else>
            <if cond="{papel}" equals="admin">
                <Text content="painel admin" />
            </if>
        </Column>
        "##
    ).unwrap();

    motor.register_component("cond", path).unwrap();

    // Estado inicial: deslogado, papel comum.
    motor.define_data("logado", "false");
    motor.define_data("usuario", "Ana");
    motor.define_data("papel", "user");

    let ev = motor.evaluated_templates.get("cond").unwrap();
    assert_eq!(ev.children.len(), 1, "só o ramo else deve aparecer");
    if let NodeType::Text { content, .. } = &ev.children[0].kind {
        assert_eq!(content, "Entre, por favor");
    } else {
        panic!("esperava o Text do else");
    }

    // Loga como admin: ramo if + comparação equals=admin.
    motor.define_data("logado", "true");
    motor.define_data("papel", "admin");

    let ev = motor.evaluated_templates.get("cond").unwrap();
    assert_eq!(ev.children.len(), 2, "ramo if verdadeiro + bloco admin");
    if let NodeType::Text { content, .. } = &ev.children[0].kind {
        assert_eq!(content, "Olá, Ana");
    } else {
        panic!("esperava o Text do if");
    }
    if let NodeType::Text { content, .. } = &ev.children[1].kind {
        assert_eq!(content, "painel admin");
    } else {
        panic!("esperava o Text do bloco admin");
    }

    std::fs::remove_file(path).ok();
}

#[test]
fn test_import_recursivo() {
    let mut motor = UiEngine::new();

    std::fs::create_dir_all("templates").ok();

    let main_path = "templates/test_imp_main.xml";
    let card_path = "templates/test_imp_card.xml";
    let badge_path = "templates/test_imp_badge.xml";

    // badge: folha, sem imports.
    std::fs::write(
        badge_path,
        r##"<Text content="[{label}]" />"##
    ).unwrap();

    // card: importa badge e o usa pelo nome.
    std::fs::write(
        card_path,
        r##"<import name="Badge" from="templates/test_imp_badge.xml" />
        <Container background="#222">
            <Column>
                <Text content="User: {name}" />
                <Badge label="ok" />
            </Column>
        </Container>"##
    ).unwrap();

    // main: importa card (que por sua vez importa badge — recursivo).
    std::fs::write(
        main_path,
        r##"<import name="Card" from="templates/test_imp_card.xml" />
        <Column>
            <Card name="Alice" />
        </Column>"##
    ).unwrap();

    // Apenas o componente de entrada é registrado.
    motor.register_component("main", main_path).unwrap();

    // Os imports recursivos devem ter sido carregados automaticamente.
    assert!(motor.parsed_templates.contains_key("Card"), "Card deveria ter sido importado");
    assert!(motor.parsed_templates.contains_key("Badge"), "Badge deveria ter sido importado recursivamente");

    let evaluated = motor.evaluated_templates.get("main").unwrap();
    assert_eq!(evaluated.kind, NodeType::Column);
    // O Card expande para um Container; o import declarado não deve virar filho visível.
    assert_eq!(evaluated.children.len(), 1);
    let card = &evaluated.children[0];
    assert_eq!(card.kind, NodeType::Container);

    let inner_col = &card.children[0];
    assert_eq!(inner_col.kind, NodeType::Column);
    // Column interna: Text "User: Alice" + Badge expandido para Text "[ok]".
    assert_eq!(inner_col.children.len(), 2);
    if let NodeType::Text { content, .. } = &inner_col.children[0].kind {
        assert_eq!(content, "User: Alice");
    } else {
        panic!("Esperava Text 'User: Alice'");
    }
    if let NodeType::Text { content, .. } = &inner_col.children[1].kind {
        assert_eq!(content, "[ok]");
    } else {
        panic!("Esperava Badge expandido em Text '[ok]'");
    }

    std::fs::remove_file(main_path).ok();
    std::fs::remove_file(card_path).ok();
    std::fs::remove_file(badge_path).ok();
}

#[test]
fn test_componente_por_nome() {
    let mut motor = UiEngine::new();

    std::fs::create_dir_all("templates").ok();

    let main_path = "templates/test_main_comp.xml";
    let card_path = "templates/test_card_comp.xml";

    std::fs::write(
        card_path,
        r##"<Container background="#222"><Text content="User: {name}" /></Container>"##
    ).unwrap();

    // Reuse via the component's own tag name instead of <Include>
    std::fs::write(
        main_path,
        r##"
        <Column>
            <UserCard name="Alice" />
            <UserCard name="Charlie" />
        </Column>
        "##
    ).unwrap();

    // The registered name must match the tag used in the XML.
    motor.register_component("UserCard", card_path).unwrap();
    motor.register_component("test_main_comp", main_path).unwrap();

    let evaluated = motor.evaluated_templates.get("test_main_comp").unwrap();
    assert_eq!(evaluated.kind, NodeType::Column);
    assert_eq!(evaluated.children.len(), 2);

    let first_child = &evaluated.children[0];
    assert_eq!(first_child.kind, NodeType::Container);
    if let NodeType::Text { content, .. } = &first_child.children[0].kind {
        assert_eq!(content, "User: Alice");
    } else {
        panic!("Component first child should contain text 'User: Alice'");
    }

    if let NodeType::Text { content, .. } = &evaluated.children[1].children[0].kind {
        assert_eq!(content, "User: Charlie");
    } else {
        panic!("Component second child should contain text 'User: Charlie'");
    }

    std::fs::remove_file(main_path).ok();
    std::fs::remove_file(card_path).ok();
}

#[test]
fn test_foreach_com_componente() {
    let mut motor = UiEngine::new();

    std::fs::create_dir_all("templates").ok();

    let main_path = "templates/test_lista.xml";
    let card_path = "templates/test_cartao.xml";

    // Componente reutilizável que recebe props.
    std::fs::write(
        card_path,
        r##"<Container background="#222"><Text content="{nome} - {cargo}" /></Container>"##
    ).unwrap();

    // Usa o componente pelo nome dentro de um ForEach, passando campos como props.
    std::fs::write(
        main_path,
        r##"
        <Column>
            <ForEach items="membros" var="m">
                <Cartao nome="{m.nome}" cargo="{m.cargo}" />
            </ForEach>
        </Column>
        "##
    ).unwrap();

    motor.register_component("Cartao", card_path).unwrap();
    motor.register_component("test_lista", main_path).unwrap();

    let data = r#"[
        {"nome": "Ana", "cargo": "Dev"},
        {"nome": "Bruno", "cargo": "Design"}
    ]"#;
    motor.define_data("membros", data);

    let evaluated = motor.evaluated_templates.get("test_lista").unwrap();
    assert_eq!(evaluated.kind, NodeType::Column);
    assert_eq!(evaluated.children.len(), 2);

    // Cada iteração do loop deve produzir o Container do componente,
    // com as props já substituídas pelos valores do item.
    let primeiro = &evaluated.children[0];
    assert_eq!(primeiro.kind, NodeType::Container);
    if let NodeType::Text { content, .. } = &primeiro.children[0].kind {
        assert_eq!(content, "Ana - Dev");
    } else {
        panic!("Esperava Text dentro do primeiro cartão");
    }

    if let NodeType::Text { content, .. } = &evaluated.children[1].children[0].kind {
        assert_eq!(content, "Bruno - Design");
    } else {
        panic!("Esperava Text dentro do segundo cartão");
    }

    std::fs::remove_file(main_path).ok();
    std::fs::remove_file(card_path).ok();
}

#[test]
fn test_navegacao_historico() {
    let mut motor = UiEngine::new();

    motor.set_initial_screen("home");
    assert_eq!(motor.current_screen.as_deref(), Some("home"));

    motor.navigate_to("config");
    motor.navigate_to("perfil");
    assert_eq!(motor.current_screen.as_deref(), Some("perfil"));

    // NavigateBack desempilha o histórico na ordem inversa.
    motor.navigate_back();
    assert_eq!(motor.current_screen.as_deref(), Some("config"));
    motor.navigate_back();
    assert_eq!(motor.current_screen.as_deref(), Some("home"));

    // Histórico vazio: navigate_back não muda a tela.
    motor.navigate_back();
    assert_eq!(motor.current_screen.as_deref(), Some("home"));

    // Navigate para a tela já ativa não empilha duplicado.
    motor.navigate_to("home");
    motor.navigate_back();
    assert_eq!(motor.current_screen.as_deref(), Some("home"));
}

#[test]
fn test_foreach() {
    let mut motor = UiEngine::new();
    
    let path = "templates/test_foreach.xml";
    std::fs::create_dir_all("templates").ok();
    std::fs::write(
        path,
        r##"
        <Column>
            <ForEach items="items" var="it">
                <Text content="Item: {it.name} ({it.val})" />
            </ForEach>
        </Column>
        "##
    ).unwrap();

    motor.register_component("test_for", path).unwrap();
    
    let data = r#"[
        {"name": "X", "val": "1"},
        {"name": "Y", "val": "2"}
    ]"#;
    motor.define_data("items", data);

    let evaluated = motor.evaluated_templates.get("test_for").unwrap();
    assert_eq!(evaluated.kind, NodeType::Column);
    assert_eq!(evaluated.children.len(), 2);

    if let NodeType::Text { content, .. } = &evaluated.children[0].kind {
        assert_eq!(content, "Item: X (1)");
    } else {
        panic!("First child should be Text Item: X (1)");
    }

    if let NodeType::Text { content, .. } = &evaluated.children[1].kind {
        assert_eq!(content, "Item: Y (2)");
    } else {
        panic!("Second child should be Text Item: Y (2)");
    }

    std::fs::remove_file(path).ok();
}


// --- Nested components: behavior composition -------------------------------

use xml_ui::{Component, Context, Template, EngineMessage};

/// Child component with its own behavior. Its button action is `ping`.
struct ChildComp;
impl Component for ChildComp {
    fn name(&self) -> &str { "ChildComp" }
    fn template(&self) -> Template {
        Template::Inline(r#"<Container><Button text="C" onClick="ping" /></Container>"#.into())
    }
    fn update(&mut self, action: &str, _v: Option<&str>, ctx: &mut Context) {
        if action == "ping" { ctx.set("child_pinged", "true"); }
    }
}

/// Parent owns ChildComp and references it in its own template.
struct ParentComp;
impl Component for ParentComp {
    fn name(&self) -> &str { "parent" }
    fn template(&self) -> Template {
        Template::Inline(
            r#"<Container><Button text="P" onClick="parent_act" /><ChildComp /></Container>"#.into(),
        )
    }
    fn update(&mut self, action: &str, _v: Option<&str>, ctx: &mut Context) {
        if action == "parent_act" { ctx.set("parent_acted", "true"); }
    }
    fn children(&self) -> Vec<Box<dyn Component>> {
        vec![Box::new(ChildComp)]
    }
}

/// Collects every `Button.on_click` in an evaluated tree.
fn collect_clicks(node: &UiNode, out: &mut Vec<String>) {
    if let NodeType::Button { on_click: Some(a), .. } = &node.kind {
        out.push(a.clone());
    }
    for c in &node.children {
        collect_clicks(c, out);
    }
}

#[test]
fn test_nested_component_action_namespacing() {
    let mut motor = UiEngine::new();
    motor.register(Box::new(ParentComp)).unwrap();
    motor.set_initial_screen("parent");

    // Both the child template (registered in cascade) and the parent exist.
    assert!(motor.parsed_templates.contains_key("parent"));
    assert!(motor.parsed_templates.contains_key("ChildComp"));

    // The child's action got namespaced; the parent's stayed plain.
    let evaluated = motor.evaluated_templates.get("parent").unwrap();
    let mut clicks = Vec::new();
    collect_clicks(evaluated, &mut clicks);
    assert!(clicks.contains(&"parent_act".to_string()), "got {:?}", clicks);
    assert!(clicks.contains(&"ChildComp::ping".to_string()), "got {:?}", clicks);
}

#[test]
fn test_nested_component_action_routing() {
    let mut motor = UiEngine::new();
    motor.register(Box::new(ParentComp)).unwrap();
    motor.set_initial_screen("parent");

    // A namespaced action reaches the child's update, not the parent's.
    motor.dispatch(&EngineMessage::XmlClick("ChildComp::ping".into())).unwrap();
    assert_eq!(motor.get_data("child_pinged").map(String::as_str), Some("true"));
    assert_eq!(motor.get_data("parent_acted"), None);

    // A plain action falls back to the active screen (the parent).
    motor.dispatch(&EngineMessage::XmlClick("parent_act".into())).unwrap();
    assert_eq!(motor.get_data("parent_acted").map(String::as_str), Some("true"));
}

/// Helper: extract the `color` of an evaluated Text node.
fn text_color(node: &NodeType) -> Option<String> {
    if let NodeType::Text { color, .. } = node {
        color.clone()
    } else {
        None
    }
}

#[test]
fn test_link_scoped_stylesheet() {
    let mut motor = UiEngine::new();
    std::fs::create_dir_all("templates").ok();

    // Global sheet: applies everywhere.
    let global_iss = "templates/test_global.iss";
    std::fs::write(global_iss, ".box { padding: 5; color: #111111; }").unwrap();

    // Scoped sheet: only component A links it. It overrides `.box`'s padding
    // and adds a `.scoped` class.
    let scoped_iss = "templates/test_scoped.iss";
    std::fs::write(scoped_iss, ".box { padding: 9; } .scoped { color: #abcabc; }").unwrap();

    // A links the scoped sheet (as a top-level sibling, before its root, to
    // exercise the <link> hoisting in parse_xml).
    let a_path = "templates/test_scoped_a.xml";
    std::fs::write(
        a_path,
        r##"
        <link rel="stylesheet" href="templates/test_scoped.iss" />
        <Text class="box scoped" content="A" />
        "##,
    ).unwrap();

    // B does not link anything: it only sees the global sheet.
    let b_path = "templates/test_scoped_b.xml";
    std::fs::write(b_path, r##"<Text class="box scoped" content="B" />"##).unwrap();

    motor.load_stylesheet(global_iss).unwrap();
    motor.register_component("a", a_path).unwrap();
    motor.register_component("b", b_path).unwrap();

    let a = motor.evaluated_templates.get("a").unwrap();
    let b = motor.evaluated_templates.get("b").unwrap();

    // A: scoped `.box` overrides padding (9 vs global 5); `.scoped` provides color.
    assert_eq!(a.padding.as_deref(), Some("9"), "scoped class should override global padding");
    assert_eq!(text_color(&a.kind).as_deref(), Some("#abcabc"), "scoped class color applies in A");

    // B: only the global `.box` applies; `.scoped` is invisible outside A's scope.
    assert_eq!(b.padding.as_deref(), Some("5"), "B uses global padding");
    assert_eq!(text_color(&b.kind).as_deref(), Some("#111111"), "B uses global color; scoped class has no effect");

    std::fs::remove_file(global_iss).ok();
    std::fs::remove_file(scoped_iss).ok();
    std::fs::remove_file(a_path).ok();
    std::fs::remove_file(b_path).ok();
}

#[test]
fn test_inline_attribute_wins_over_class() {
    let mut motor = UiEngine::new();
    std::fs::create_dir_all("templates").ok();

    let iss = "templates/test_inline.iss";
    std::fs::write(iss, ".tag { color: #aaaaaa; padding: 3; }").unwrap();

    let path = "templates/test_inline.xml";
    // Inline color overrides the class; padding falls back to the class.
    std::fs::write(path, r##"<Text class="tag" content="x" color="#ff0000" />"##).unwrap();

    motor.load_stylesheet(iss).unwrap();
    motor.register_component("inline", path).unwrap();

    let n = motor.evaluated_templates.get("inline").unwrap();
    assert_eq!(text_color(&n.kind).as_deref(), Some("#ff0000"), "inline color wins");
    assert_eq!(n.padding.as_deref(), Some("3"), "padding comes from the class");

    std::fs::remove_file(iss).ok();
    std::fs::remove_file(path).ok();
}

#[test]
fn test_link_rel_import() {
    let mut motor = UiEngine::new();
    std::fs::create_dir_all("templates").ok();

    let child = "templates/test_li_child.xml";
    std::fs::write(child, r##"<Text content="child:{x}" />"##).unwrap();

    let parent = "templates/test_li_parent.xml";
    // Declarative import via <link>; the component is then referenced by name.
    std::fs::write(
        parent,
        r##"
        <link rel="import" href="templates/test_li_child.xml" as="ChildLink" />
        <Column>
            <ChildLink x="42" />
        </Column>
        "##,
    ).unwrap();

    motor.register_component("parent", parent).unwrap();

    // The imported component must be registered and inlined with its prop.
    assert!(motor.parsed_templates.contains_key("ChildLink"), "import should register the component");
    let ev = motor.evaluated_templates.get("parent").unwrap();
    assert_eq!(ev.children.len(), 1);
    if let NodeType::Text { content, .. } = &ev.children[0].kind {
        assert_eq!(content, "child:42");
    } else {
        panic!("expected the imported Text");
    }

    std::fs::remove_file(child).ok();
    std::fs::remove_file(parent).ok();
}

#[test]
fn test_link_rel_data() {
    let mut motor = UiEngine::new();
    std::fs::create_dir_all("templates").ok();

    let data = "templates/test_data.json";
    std::fs::write(data, r##"{ "title": "Olá", "users": [ {"name": "Ana"}, {"name": "Bob"} ] }"##).unwrap();

    let tpl = "templates/test_data.xml";
    std::fs::write(
        tpl,
        r##"
        <link rel="data" href="templates/test_data.json" as="app" />
        <Column>
            <Text content="{app.title}" />
            <ForEach items="app.users" var="u">
                <Text content="{u.name}" />
            </ForEach>
        </Column>
        "##,
    ).unwrap();

    motor.register_component("datacomp", tpl).unwrap();

    // Object field flattened to `app.title`.
    assert_eq!(motor.get_data("app.title").map(String::as_str), Some("Olá"));

    let ev = motor.evaluated_templates.get("datacomp").unwrap();
    // 1 title + 2 ForEach-expanded users.
    assert_eq!(ev.children.len(), 3, "title + two users");
    let names: Vec<String> = ev.children.iter().filter_map(|c| {
        if let NodeType::Text { content, .. } = &c.kind { Some(content.clone()) } else { None }
    }).collect();
    assert_eq!(names, vec!["Olá", "Ana", "Bob"]);

    std::fs::remove_file(data).ok();
    std::fs::remove_file(tpl).ok();
}

#[test]
fn test_link_rel_theme() {
    let mut motor = UiEngine::new();
    std::fs::create_dir_all("templates").ok();

    let theme = "templates/test_theme.json";
    std::fs::write(
        theme,
        r##"{ "name": "test", "background": "#102030", "text": "#FFFFFF", "primary": "#A0B0C0", "success": "#00FF00", "danger": "#FF0000" }"##,
    ).unwrap();

    let tpl = "templates/test_theme.xml";
    std::fs::write(
        tpl,
        r##"
        <link rel="theme" href="templates/test_theme.json" />
        <Text content="x" />
        "##,
    ).unwrap();

    // Default theme before loading anything is Dark.
    assert!(motor.custom_theme.is_none());

    motor.register_component("themecomp", tpl).unwrap();

    assert!(motor.custom_theme.is_some(), "theme link should set a custom theme");
    let bg = motor.theme().palette().background;
    assert!((bg.r - 16.0 / 255.0).abs() < 1e-6, "background red channel");
    assert!((bg.g - 32.0 / 255.0).abs() < 1e-6, "background green channel");
    assert!((bg.b - 48.0 / 255.0).abs() < 1e-6, "background blue channel");

    std::fs::remove_file(theme).ok();
    std::fs::remove_file(tpl).ok();
}
