use glacier_ui::{GlacierUI, UiNode, NodeType};

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
    let mut motor = GlacierUI::new();
    
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
    let mut motor = GlacierUI::new();
    
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
    let mut motor = GlacierUI::new();

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
    let mut motor = GlacierUI::new();

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
    let mut motor = GlacierUI::new();

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
    let mut motor = GlacierUI::new();

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
    let mut motor = GlacierUI::new();

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
    let mut motor = GlacierUI::new();
    
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

use glacier_ui::{Component, Context, Template, EngineMessage};

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
    let mut motor = GlacierUI::new();
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
    let mut motor = GlacierUI::new();
    motor.register(Box::new(ParentComp)).unwrap();
    motor.set_initial_screen("parent");

    // A namespaced action reaches the child's update, not the parent's.
    let _ = motor.dispatch(&EngineMessage::UiClick("ChildComp::ping".into()));
    assert_eq!(motor.get_data("child_pinged").map(String::as_str), Some("true"));
    assert_eq!(motor.get_data("parent_acted"), None);

    // A plain action falls back to the active screen (the parent).
    let _ = motor.dispatch(&EngineMessage::UiClick("parent_act".into()));
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
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();

    // Global sheet: applies everywhere.
    let global_gss = "templates/test_global.gss";
    std::fs::write(global_gss, ".box { padding: 5; color: #111111; }").unwrap();

    // Scoped sheet: only component A links it. It overrides `.box`'s padding
    // and adds a `.scoped` class.
    let scoped_gss = "templates/test_scoped.gss";
    std::fs::write(scoped_gss, ".box { padding: 9; } .scoped { color: #abcabc; }").unwrap();

    // A links the scoped sheet (as a top-level sibling, before its root, to
    // exercise the <link> hoisting in parse_xml).
    let a_path = "templates/test_scoped_a.xml";
    std::fs::write(
        a_path,
        r##"
        <link rel="stylesheet" href="templates/test_scoped.gss" />
        <Text class="box scoped" content="A" />
        "##,
    ).unwrap();

    // B does not link anything: it only sees the global sheet.
    let b_path = "templates/test_scoped_b.xml";
    std::fs::write(b_path, r##"<Text class="box scoped" content="B" />"##).unwrap();

    motor.load_stylesheet(global_gss).unwrap();
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

    std::fs::remove_file(global_gss).ok();
    std::fs::remove_file(scoped_gss).ok();
    std::fs::remove_file(a_path).ok();
    std::fs::remove_file(b_path).ok();
}

#[test]
fn test_inline_style_block_is_scoped() {
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();

    // Global sheet seen by everyone.
    let global_gss = "templates/test_istyle_global.gss";
    std::fs::write(global_gss, ".box { padding: 5; color: #111111; }").unwrap();

    // A declares an inline <style> that overrides `.box` and adds `.scoped`.
    let a_path = "templates/test_istyle_a.xml";
    std::fs::write(
        a_path,
        r##"
        <style>
            .box { padding: 9; }
            .scoped { color: #abcabc; }
        </style>
        <Text class="box scoped" content="A" />
        "##,
    ).unwrap();

    // B declares nothing: it only sees the global sheet.
    let b_path = "templates/test_istyle_b.xml";
    std::fs::write(b_path, r##"<Text class="box scoped" content="B" />"##).unwrap();

    motor.load_stylesheet(global_gss).unwrap();
    motor.register_component("a", a_path).unwrap();
    motor.register_component("b", b_path).unwrap();

    let a = motor.evaluated_templates.get("a").unwrap();
    let b = motor.evaluated_templates.get("b").unwrap();

    // A: inline `.box` overrides the global padding; `.scoped` provides a color.
    assert_eq!(a.padding.as_deref(), Some("9"), "inline <style> overrides global padding");
    assert_eq!(text_color(&a.kind).as_deref(), Some("#abcabc"), "inline scoped class color applies in A");

    // B: only the global `.box` applies; A's inline classes are invisible here.
    assert_eq!(b.padding.as_deref(), Some("5"), "B uses global padding");
    assert_eq!(text_color(&b.kind).as_deref(), Some("#111111"), "B uses global color; inline class has no effect");

    std::fs::remove_file(global_gss).ok();
    std::fs::remove_file(a_path).ok();
    std::fs::remove_file(b_path).ok();
}

#[test]
fn test_inline_style_overrides_linked_by_document_order() {
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();

    // A linked sheet sets the color; a later inline <style> overrides it,
    // because scoped sheets layer in document order (last one wins).
    let linked = "templates/test_istyle_order.gss";
    std::fs::write(linked, ".tag { color: #aaaaaa; padding: 3; }").unwrap();

    let path = "templates/test_istyle_order.xml";
    std::fs::write(
        path,
        r##"
        <link rel="stylesheet" href="templates/test_istyle_order.gss" />
        <style>.tag { color: #bbbbbb; }</style>
        <Text class="tag" content="x" />
        "##,
    ).unwrap();

    motor.register_component("ord", path).unwrap();

    let n = motor.evaluated_templates.get("ord").unwrap();
    assert_eq!(text_color(&n.kind).as_deref(), Some("#bbbbbb"), "later inline <style> wins over the linked sheet");
    assert_eq!(n.padding.as_deref(), Some("3"), "padding still comes from the linked sheet");

    std::fs::remove_file(linked).ok();
    std::fs::remove_file(path).ok();
}

#[test]
fn test_kdl_inline_style_parses_to_style_node() {
    // A multi-line `style` argument carrying `.gss` source is an inline,
    // scoped sheet; a single-line path stays a `stylesheet` link.
    let kdl = r#"
        style """
        .card { background: #1E1E2E; padding: 16; }
        """
        style "styles/app.gss"

        Container class="card" {
            Text "hi"
        }
    "#;
    let ast = glacier_ui::parse_kdl(kdl).unwrap();

    let mut found_inline = false;
    let mut found_linked = false;
    for child in &ast.children {
        match &child.kind {
            NodeType::Style { css } => {
                found_inline = true;
                assert!(css.contains(".card"), "inline style keeps its gss body");
            }
            NodeType::Link { rel, href, .. } if rel == "stylesheet" => {
                found_linked = true;
                assert_eq!(href, "styles/app.gss");
            }
            _ => {}
        }
    }
    assert!(found_inline, "multi-line style arg should parse to a Style node");
    assert!(found_linked, "single-line style arg should stay a stylesheet Link");
}

#[test]
fn test_inline_style_reloads_with_template() {
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();

    let path = "templates/test_istyle_reload.xml";
    std::fs::write(
        path,
        r##"<style>.t { color: #010101; }</style><Text class="t" content="x" />"##,
    ).unwrap();
    motor.register_component("rel", path).unwrap();
    let n = motor.evaluated_templates.get("rel").unwrap();
    assert_eq!(text_color(&n.kind).as_deref(), Some("#010101"));

    // Edit the inline style; bump mtime so the reload check picks it up.
    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(
        path,
        r##"<style>.t { color: #020202; }</style><Text class="t" content="x" />"##,
    ).unwrap();
    let _ = filetime_touch(path);
    motor.check_reload();

    let n = motor.evaluated_templates.get("rel").unwrap();
    assert_eq!(text_color(&n.kind).as_deref(), Some("#020202"), "inline style rebuilds when the template reloads");

    std::fs::remove_file(path).ok();
}

/// Sets a file's mtime to now, so `check_reload` reliably sees it as changed
/// even on filesystems with coarse timestamps.
fn filetime_touch(path: &str) -> std::io::Result<()> {
    use std::time::SystemTime;
    let f = std::fs::OpenOptions::new().write(true).open(path)?;
    f.set_modified(SystemTime::now())
}

#[test]
fn test_inline_attribute_wins_over_class() {
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();

    let gss = "templates/test_inline.gss";
    std::fs::write(gss, ".tag { color: #aaaaaa; padding: 3; }").unwrap();

    let path = "templates/test_inline.xml";
    // Inline color overrides the class; padding falls back to the class.
    std::fs::write(path, r##"<Text class="tag" content="x" color="#ff0000" />"##).unwrap();

    motor.load_stylesheet(gss).unwrap();
    motor.register_component("inline", path).unwrap();

    let n = motor.evaluated_templates.get("inline").unwrap();
    assert_eq!(text_color(&n.kind).as_deref(), Some("#ff0000"), "inline color wins");
    assert_eq!(n.padding.as_deref(), Some("3"), "padding comes from the class");

    std::fs::remove_file(gss).ok();
    std::fs::remove_file(path).ok();
}

#[test]
fn test_link_rel_import() {
    let mut motor = GlacierUI::new();
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
fn test_textarea_parses_and_syncs() {
    // A `<TextArea>` parses to its own node and the engine seeds a stateful
    // editor buffer from the bound context value.
    let xml = r##"<TextArea value="dotenv" placeholder="KEY=VALUE" onChange="env_changed" />"##;
    let ast = UiNode::parse_xml(xml).unwrap();
    match &ast.kind {
        NodeType::TextArea { value_var, placeholder, on_change } => {
            assert_eq!(value_var, "dotenv");
            assert_eq!(placeholder, "KEY=VALUE");
            assert_eq!(on_change, "env_changed");
        }
        other => panic!("expected TextArea, got {other:?}"),
    }

    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();
    let tpl = "templates/test_textarea.xml";
    std::fs::write(tpl, xml).unwrap();
    motor.register_component("tacomp", tpl).unwrap();
    motor.define_data("dotenv", "FOO=1\nBAR=2");
    // A reevaluation seeds the editor buffer from the context without panicking.
    motor.reevaluate_all().unwrap();
    assert!(motor.render("tacomp").is_ok());

    std::fs::remove_file(tpl).ok();
}

#[test]
fn test_select_parses_and_renders() {
    // A `<Select>` parses to its own node and renders from a context JSON array,
    // marking the bound value as selected.
    let xml = r##"<Select options="repos" value="chosen" onChange="pick" placeholder="escolha" labelField="full_name" valueField="clone_url" />"##;
    let ast = UiNode::parse_xml(xml).unwrap();
    match &ast.kind {
        NodeType::Select { options, value_var, on_change, placeholder, label_field, value_field, .. } => {
            assert_eq!(options, "repos");
            assert_eq!(value_var, "chosen");
            assert_eq!(on_change, "pick");
            assert_eq!(placeholder, "escolha");
            assert_eq!(label_field, "full_name");
            assert_eq!(value_field, "clone_url");
        }
        other => panic!("expected Select, got {other:?}"),
    }

    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();
    let tpl = "templates/test_select.xml";
    std::fs::write(tpl, xml).unwrap();
    motor.register_component("selcomp", tpl).unwrap();
    motor.define_data(
        "repos",
        r##"[{"full_name":"org/a","clone_url":"https://x/a.git"},{"full_name":"org/b","clone_url":"https://x/b.git"}]"##,
    );
    motor.define_data("chosen", "https://x/b.git");
    motor.reevaluate_all().unwrap();
    assert!(motor.render("selcomp").is_ok());

    std::fs::remove_file(tpl).ok();
}

#[test]
fn test_if_else_inside_foreach() {
    // Regression: `<if>`/`<else>` nested directly under a `<ForEach>` must be
    // resolved per item (only the matching branch renders), not emitted as
    // plain nodes with both branches expanded.
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();

    let data = "templates/test_ifforeach.json";
    std::fs::write(
        data,
        r##"{ "rows": [ {"filler":"0","name":"api"}, {"filler":"1","name":"x"}, {"filler":"0","name":"web"} ] }"##,
    )
    .unwrap();

    let tpl = "templates/test_ifforeach.xml";
    std::fs::write(
        tpl,
        r##"
        <link rel="data" href="templates/test_ifforeach.json" as="d" />
        <Column>
            <ForEach items="d.rows" var="r">
                <if cond="{r.filler}" equals="1">
                    <Text content="GAP" />
                </if>
                <else>
                    <Text content="{r.name}" />
                </else>
            </ForEach>
        </Column>
        "##,
    )
    .unwrap();

    motor.register_component("ifforeach", tpl).unwrap();

    let ev = motor.evaluated_templates.get("ifforeach").unwrap();
    let texts: Vec<String> = ev
        .children
        .iter()
        .filter_map(|c| {
            if let NodeType::Text { content, .. } = &c.kind {
                Some(content.clone())
            } else {
                None
            }
        })
        .collect();
    // Exactly one node per item, picking the right branch.
    assert_eq!(texts, vec!["api", "GAP", "web"]);

    std::fs::remove_file(data).ok();
    std::fs::remove_file(tpl).ok();
}

#[test]
fn test_link_rel_data() {
    let mut motor = GlacierUI::new();
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
    let mut motor = GlacierUI::new();
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

// ── New widgets & async bridge (v0.2) ───────────────────────────────────────

#[test]
fn parses_new_widget_tags() {
    let xml = r##"
    <Column>
        <Scrollable direction="vertical"><Text content="a" /></Scrollable>
        <Checkbox label="Remember" checked="remember" onToggle="toggle_remember" />
        <Toggle label="Enabled" checked="enabled" onToggle="toggle_enabled" />
        <Rule direction="horizontal" />
        <Svg source="icons/rocket.svg" color="#89B4FA" />
    </Column>
    "##;
    let ast = UiNode::parse_xml(xml).unwrap();
    let kinds: Vec<&NodeType> = ast.children.iter().map(|c| &c.kind).collect();
    assert!(matches!(kinds[0], NodeType::Scrollable { .. }));
    assert!(matches!(kinds[1], NodeType::Checkbox { .. }));
    assert!(matches!(kinds[2], NodeType::Toggle { .. }));
    assert!(matches!(kinds[3], NodeType::Rule { horizontal: true }));
    assert!(matches!(kinds[4], NodeType::Svg { .. }));

    if let NodeType::Checkbox { label, checked_var, on_toggle } = &ast.children[1].kind {
        assert_eq!(label, "Remember");
        assert_eq!(checked_var, "remember");
        assert_eq!(on_toggle, "toggle_remember");
    } else {
        panic!("expected checkbox");
    }
}

#[test]
fn parses_font_gradient_text_align() {
    let xml = r##"<Text content="Hi" font="mono" gradient="180 #000000 #FFFFFF" textAlign="center" />"##;
    let ast = UiNode::parse_xml(xml).unwrap();
    assert_eq!(ast.font.as_deref(), Some("mono"));
    assert_eq!(ast.gradient.as_deref(), Some("180 #000000 #FFFFFF"));
    assert_eq!(ast.text_align.as_deref(), Some("center"));
}

#[test]
fn context_patch_merges_into_context() {
    use glacier_ui::EngineMessage;
    let mut motor = GlacierUI::new();
    let _task = motor.dispatch(&EngineMessage::ContextPatch(vec![
        ("status".into(), "ok".into()),
        ("count".into(), "3".into()),
    ]));
    assert_eq!(motor.get_data("status").map(String::as_str), Some("ok"));
    assert_eq!(motor.get_data("count").map(String::as_str), Some("3"));
}

#[test]
fn gss_supports_font_and_text_align() {
    use glacier_ui::StyleSheet;
    let sheet = StyleSheet::parse(".mono { font: mono; text-align: center; }").unwrap();
    let rule = sheet.rules.get("mono").unwrap();
    assert_eq!(rule.font.as_deref(), Some("mono"));
    assert_eq!(rule.text_align.as_deref(), Some("center"));
}

#[test]
fn test_directives_as_attributes() {
    let mut motor = GlacierUI::new();

    std::fs::create_dir_all("templates").ok();
    let path = "templates/test_directives_attr.xml";
    std::fs::write(
        path,
        r##"
        <Column>
            <Text content="Olá, {usuario}" if="{logado}" />
            <Text content="Entre, por favor" senao />
            <Text content="painel admin" if="{papel}" equals="admin" />
            <Text content="painel comum" if="{papel}" notEquals="admin" />
        </Column>
        "##
    ).unwrap();

    motor.register_component("cond_attr", path).unwrap();

    // Estado inicial: deslogado, papel comum
    motor.define_data("logado", "false");
    motor.define_data("usuario", "Ana");
    motor.define_data("papel", "user");

    let ev = motor.evaluated_templates.get("cond_attr").unwrap();
    // O primeiro Text (if) é ocultado. O segundo Text (senao) é exibido.
    // O terceiro (if papel equals admin) é ocultado. O quarto (if papel notEquals admin) é exibido.
    assert_eq!(ev.children.len(), 2);
    if let NodeType::Text { content, .. } = &ev.children[0].kind {
        assert_eq!(content, "Entre, por favor");
    } else {
        panic!("esperava o Text do senao");
    }
    if let NodeType::Text { content, .. } = &ev.children[1].kind {
        assert_eq!(content, "painel comum");
    } else {
        panic!("esperava o Text de papel comum");
    }

    // Logado como admin
    motor.define_data("logado", "true");
    motor.define_data("papel", "admin");

    let ev = motor.evaluated_templates.get("cond_attr").unwrap();
    // O primeiro Text (if) é exibido. O segundo (senao) é ocultado.
    // O terceiro (if papel equals admin) é exibido. O quarto (if papel notEquals admin) é ocultado.
    assert_eq!(ev.children.len(), 2);
    if let NodeType::Text { content, .. } = &ev.children[0].kind {
        assert_eq!(content, "Olá, Ana");
    } else {
        panic!("esperava o Text do if");
    }
    if let NodeType::Text { content, .. } = &ev.children[1].kind {
        assert_eq!(content, "painel admin");
    } else {
        panic!("esperava o Text do admin");
    }

    std::fs::remove_file(path).ok();
}

#[test]
fn test_precedence_foreach_if_attributes() {
    let mut motor = GlacierUI::new();

    std::fs::create_dir_all("templates").ok();
    let path = "templates/test_precedence.xml";
    std::fs::write(
        path,
        r##"
        <Column>
            <Text content="Item: {u.nome}" for-each="usuarios" var="u" if="{u.ativo}" />
        </Column>
        "##
    ).unwrap();

    motor.register_component("precedence", path).unwrap();

    let json = serde_json::json!([
        { "nome": "Clara", "ativo": "true" },
        { "nome": "Sophia", "ativo": "false" },
        { "nome": "Mateus", "ativo": "true" }
    ]).to_string();
    motor.define_data("usuarios", &json);

    let ev = motor.evaluated_templates.get("precedence").unwrap();
    // Deve renderizar apenas "Clara" e "Mateus", pois "Sophia" tem ativo="false".
    assert_eq!(ev.children.len(), 2);
    if let NodeType::Text { content, .. } = &ev.children[0].kind {
        assert_eq!(content, "Item: Clara");
    } else {
        panic!("esperava o primeiro item");
    }
    if let NodeType::Text { content, .. } = &ev.children[1].kind {
        assert_eq!(content, "Item: Mateus");
    } else {
        panic!("esperava o segundo item");
    }

    std::fs::remove_file(path).ok();
}

// ---------------------------------------------------------------------------
// KDL parser
// ---------------------------------------------------------------------------

#[test]
fn test_kdl_basic_layout() {
    let kdl = r##"
    Container padding=15 width=200 background="#FFFFFF" {
        Column spacing=10 {
            Text "Hello World" size=20 bold
            Button "Click Me" onClick="btn_click"
        }
    }
    "##;

    let ast = glacier_ui::parse_kdl(kdl).unwrap();

    assert_eq!(ast.kind, NodeType::Container);
    assert_eq!(ast.padding.as_deref(), Some("15"));
    assert_eq!(ast.width.as_deref(), Some("200"));
    assert_eq!(ast.background.as_deref(), Some("#FFFFFF"));

    assert_eq!(ast.children.len(), 1);
    let column = &ast.children[0];
    assert_eq!(column.kind, NodeType::Column);
    assert_eq!(column.spacing, Some(10.0));
    assert_eq!(column.children.len(), 2);

    if let NodeType::Text { content, size, bold, .. } = &column.children[0].kind {
        assert_eq!(content, "Hello World");
        assert_eq!(*size, Some(20.0));
        assert!(bold, "o flag abreviado `bold` deve virar bold=true");
    } else {
        panic!("primeiro filho do Column deveria ser Text");
    }

    if let NodeType::Button { text, on_click, .. } = &column.children[1].kind {
        assert_eq!(text, "Click Me");
        assert_eq!(on_click.as_deref(), Some("btn_click"));
    } else {
        panic!("segundo filho do Column deveria ser Button");
    }
}

#[test]
fn test_kdl_declarations_hoisted() {
    // theme/style/import/data viram nós Link/Import anexados como filhos da raiz.
    let kdl = r#"
    theme "styles/theme.json"
    style "styles/estilos.gss"
    import "PerfilCard" from="templates/perfil_card.xml"
    data "styles/dados.json" as="perfil"

    Container {
        Text "ola"
    }
    "#;

    let ast = glacier_ui::parse_kdl(kdl).unwrap();
    assert_eq!(ast.kind, NodeType::Container);

    let mut found_theme = false;
    let mut found_style = false;
    let mut found_data = false;
    let mut found_import = false;
    for child in &ast.children {
        match &child.kind {
            NodeType::Link { rel, href, name } if rel == "theme" => {
                found_theme = true;
                assert_eq!(href, "styles/theme.json");
                assert!(name.is_none());
            }
            NodeType::Link { rel, href, .. } if rel == "stylesheet" => {
                found_style = true;
                assert_eq!(href, "styles/estilos.gss");
            }
            NodeType::Link { rel, href, name } if rel == "data" => {
                found_data = true;
                assert_eq!(href, "styles/dados.json");
                assert_eq!(name.as_deref(), Some("perfil"));
            }
            NodeType::Import { name, from } => {
                found_import = true;
                assert_eq!(name, "PerfilCard");
                assert_eq!(from, "templates/perfil_card.xml");
            }
            _ => {}
        }
    }
    assert!(found_theme && found_style && found_data && found_import,
        "todas as declarações deveriam ser anexadas como filhos da raiz");
}

#[test]
fn test_kdl_flow_control_attributes() {
    // if/else/for-each como atributos, igual ao XML.
    // `else` aceita o flag abreviado (bare) ou a forma KDL v2 `else=#true`.
    let kdl = r#"
    Column {
        Text "Bem-vindo, {usuario}" if="{logado}"
        Text "Faça login" else
        CartaoUsuario for-each="usuarios" var="u" nome="{u.nome}"
    }
    "#;

    let ast = glacier_ui::parse_kdl(kdl).unwrap();
    let c = &ast.children;
    assert_eq!(c.len(), 3);

    assert_eq!(c[0].if_cond.as_deref(), Some("{logado}"));
    assert!(c[1].is_else, "o flag `else` deveria marcar is_else");
    assert_eq!(c[2].for_each.as_deref(), Some("usuarios"));
    assert_eq!(c[2].for_each_var.as_deref(), Some("u"));
    if let NodeType::Component { name, props } = &c[2].kind {
        assert_eq!(name, "CartaoUsuario");
        assert_eq!(props.get("nome").map(String::as_str), Some("{u.nome}"));
    } else {
        panic!("esperava um Component CartaoUsuario");
    }
}

#[test]
fn test_kdl_script_block_stripped() {
    // O bloco `script { ... }` (corpo Rust) é removido antes do parse KDL.
    let kdl = r#"
    Container {
        Text "ola"
    }

    script {
        fn incrementar(self) {
            self.contador += 1;
        }
    }
    "#;

    let ast = glacier_ui::parse_kdl(kdl).unwrap();
    assert_eq!(ast.kind, NodeType::Container);
    assert_eq!(ast.children.len(), 1);
}

#[test]
fn test_kdl_file_end_to_end() {
    // Um arquivo .kdl roda no motor exatamente como um .xml: o parser é
    // escolhido pela extensão.
    let mut motor = GlacierUI::new();

    std::fs::create_dir_all("templates").ok();
    let path = "templates/test_e2e.kdl";
    std::fs::write(
        path,
        r#"
        Column {
            Text "Contador: {contador}" size=24 bold
            Button "+" onClick="incrementar"
        }
        "#,
    ).unwrap();

    motor.register_component("kdl_e2e", path).unwrap();
    motor.define_data("contador", "7");

    let ev = motor.evaluated_templates.get("kdl_e2e").unwrap();
    assert_eq!(ev.kind, NodeType::Column);
    if let NodeType::Text { content, size, bold, .. } = &ev.children[0].kind {
        assert_eq!(content, "Contador: 7");
        assert_eq!(*size, Some(24.0));
        assert!(bold);
    } else {
        panic!("esperava o Text interpolado");
    }

    std::fs::remove_file(path).ok();
}

#[test]
fn test_kdl_import_and_scoped_stylesheet() {
    // Usa os arquivos reais do exemplo `painel_kdl`: o template importa um
    // componente KDL e declara uma folha .gss escopada.
    let mut motor = GlacierUI::new();
    motor.register_component("painel_kdl", "templates/painel_kdl.kdl").unwrap();

    // O `import "CartaoKdl"` deve ter registrado o componente importado.
    assert!(
        motor.parsed_templates.contains_key("CartaoKdl"),
        "o import do CartaoKdl deveria ter sido carregado"
    );

    // Árvore avaliada: a classe escopada `.painel` foi resolvida nos campos da
    // raiz (após a avaliação `class` vira None — as classes viram atributos).
    let ev = motor.evaluated_templates.get("painel_kdl").unwrap();
    assert_eq!(ev.padding.as_deref(), Some("30"), ".painel (escopada) aplica padding");
    assert_eq!(ev.background.as_deref(), Some("#11111B"), ".painel aplica background");

    // Os CartaoKdl foram inlinados: cada um vira o Container raiz do cartão
    // (width 320, background #181825 vindos dos atributos inline do componente).
    // O template usa as duas formas (continuação com `\` e em múltiplas linhas).
    let cards = collect_cards(ev);
    assert_eq!(cards.len(), 4, "esperava 4 cartões inlinados (formas `\\` + multilinha)");
    assert_eq!(cards[0].width.as_deref(), Some("320"), "o cartão importado aplica seu width inline");
    assert_eq!(cards[0].background.as_deref(), Some("#181825"));

    // As props nome/cargo foram interpoladas no contexto local de cada cartão.
    let textos = collect_texts(ev);
    assert!(textos.iter().any(|t| t == "Clara Silva"), "prop nome deveria ser interpolada");
    assert!(textos.iter().any(|t| t == "Designer de Interface"), "prop cargo deveria ser interpolada");
    // Cartões escritos na forma multilinha (sem `\`) também foram parseados.
    assert!(textos.iter().any(|t| t == "Mateus Rocha"), "cartão multilinha (com filhos) deveria existir");
    assert!(textos.iter().any(|t| t == "Helena Dias"), "cartão multilinha (com `;`) deveria existir");
}

#[test]
fn test_kdl_multiline_node_props() {
    // Props em linhas próprias, sem `\`. O primeiro nó fecha no dedent (próximo
    // irmão); o segundo fecha explicitamente com `;`.
    let kdl = r##"
    Column {
        CartaoKdl
            nome="Mateus Rocha"
            cargo="Gerente de Produto"
            cor="#A6E3A1"
        CartaoKdl
            nome="Ana Lima"
            cor="#89B4FA";
    }
    "##;
    let ast = glacier_ui::parse_kdl(kdl).unwrap();
    assert_eq!(ast.kind, NodeType::Column);
    assert_eq!(ast.children.len(), 2, "duas referências de componente irmãs");

    let props0 = component_props(&ast.children[0]);
    assert_eq!(props0.get("nome").map(String::as_str), Some("Mateus Rocha"));
    assert_eq!(props0.get("cargo").map(String::as_str), Some("Gerente de Produto"));
    assert_eq!(props0.get("cor").map(String::as_str), Some("#A6E3A1"));

    let props1 = component_props(&ast.children[1]);
    assert_eq!(props1.get("nome").map(String::as_str), Some("Ana Lima"));
    assert_eq!(props1.get("cor").map(String::as_str), Some("#89B4FA"));
}

#[test]
fn test_kdl_multiline_node_with_children() {
    // Props em múltiplas linhas terminadas por um bloco de filhos `{ ... }`.
    let kdl = r#"
    Container
        class="card"
        padding=20 {
            Text "dentro"
        }
    "#;
    let ast = glacier_ui::parse_kdl(kdl).unwrap();
    assert_eq!(ast.kind, NodeType::Container);
    assert_eq!(ast.class.as_deref(), Some("card"));
    assert_eq!(ast.padding.as_deref(), Some("20"));
    assert_eq!(ast.children.len(), 1);
    if let NodeType::Text { content, .. } = &ast.children[0].kind {
        assert_eq!(content, "dentro");
    } else {
        panic!("esperava o Text filho");
    }
}

#[test]
fn test_kdl_backslash_continuation_still_works() {
    // A forma legada com `\` continua válida e equivale à multilinha.
    let kdl = "
    CartaoKdl \\
        nome=\"Clara Silva\" \\
        cargo=\"Engenheira\" \\
        cor=\"#89B4FA\"
    ";
    let ast = glacier_ui::parse_kdl(kdl).unwrap();
    let props = component_props(&ast);
    assert_eq!(props.get("nome").map(String::as_str), Some("Clara Silva"));
    assert_eq!(props.get("cargo").map(String::as_str), Some("Engenheira"));
    assert_eq!(props.get("cor").map(String::as_str), Some("#89B4FA"));
}

#[test]
fn test_kdl_multiline_preserves_inline_style_block() {
    // O conteúdo da string multilinha (GSS de um `style` inline) não é tocado
    // pelo pré-processador de continuação, mesmo com chaves/`:`/`;` por linha.
    let kdl = r##"
    style """
    .card {
        padding: 16;
        color: #CDD6F4;
    }
    """
    Container class="card" {
        Text "oi"
    }
    "##;
    let ast = glacier_ui::parse_kdl(kdl).unwrap();
    // O nó de estilo inline foi içado como filho da raiz, com o GSS intacto.
    let style = ast.children.iter().find_map(|c| match &c.kind {
        NodeType::Style { css } => Some(css.clone()),
        _ => None,
    }).expect("esperava um nó Style inline");
    assert!(style.contains("padding: 16;"), "o corpo GSS deve ficar intacto");
    assert!(style.contains("color: #CDD6F4;"));
}

#[test]
fn test_kdl_close_brace_then_sibling_same_line() {
    // `} node2 {` na mesma linha: o KDL exige terminador após o bloco de filhos;
    // o pré-processador quebra a linha para que ambos os nós sejam parseados.
    let kdl = r#"
    Column {
        Container {
            Text "a"
        } Container {
            Text "b"
        }
    }
    "#;
    let ast = glacier_ui::parse_kdl(kdl).unwrap();
    assert_eq!(ast.kind, NodeType::Column);
    assert_eq!(ast.children.len(), 2, "os dois Containers irmãos devem ser parseados");
    assert!(matches!(ast.children[0].kind, NodeType::Container));
    assert!(matches!(ast.children[1].kind, NodeType::Container));
    let textos = collect_texts(&ast);
    assert_eq!(textos, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn test_kdl_close_brace_chain_and_compact() {
    // Forma compacta numa só linha, com fechamentos encadeados `} }`.
    let kdl = r#"
    Row { Container { Text "x" } Container { Text "y" } }
    "#;
    let ast = glacier_ui::parse_kdl(kdl).unwrap();
    assert_eq!(ast.kind, NodeType::Row);
    assert_eq!(ast.children.len(), 2);
    assert_eq!(collect_texts(&ast), vec!["x".to_string(), "y".to_string()]);
}

#[test]
fn test_kdl_close_brace_inside_strings_not_split() {
    // Um `}` dentro de string normal e dentro de `"""` (corpo GSS) não pode
    // disparar a quebra de linha.
    let kdl = r##"
    style """
    .a { color: #fff; } .b { color: #000; }
    """
    Column {
        Text "tem } chave" class="a"
    }
    "##;
    let ast = glacier_ui::parse_kdl(kdl).unwrap();
    assert_eq!(ast.kind, NodeType::Column);
    // O Text com `}` literal no conteúdo continua um único nó intacto.
    let textos = collect_texts(&ast);
    assert_eq!(textos, vec!["tem } chave".to_string()]);
    // O style inline preservou o GSS (inclusive duas regras na mesma linha).
    let style = ast.children.iter().find_map(|c| match &c.kind {
        NodeType::Style { css } => Some(css.clone()),
        _ => None,
    }).expect("esperava o nó Style inline");
    assert!(style.contains(".a { color: #fff; } .b { color: #000; }"));
}

#[test]
fn test_kdl_builtin_bare_flag_on_own_continuation_line() {
    // Um flag bare *do framework* (`bold`) numa linha de continuação própria
    // deve ser dobrado de volta no nó acima — sem registro do cliente, e sem
    // virar um novo nó irmão que engole as propriedades seguintes.
    let kdl = r##"
    Column {
        Text "Título"
            bold
            size=24
            color="#fff"
        Text "depois"
    }
    "##;
    let ast = glacier_ui::parse_kdl(kdl).unwrap();
    assert_eq!(ast.children.len(), 2, "o flag `bold` não pode virar um nó extra");
    if let NodeType::Text { content, size, bold, color } = &ast.children[0].kind {
        assert_eq!(content, "Título");
        assert!(bold, "o flag bare `bold` deve virar bold=true");
        assert_eq!(*size, Some(24.0), "o size sobrevive ao fold");
        assert_eq!(color.as_deref(), Some("#fff"), "a cor sobrevive ao fold");
    } else {
        panic!("o primeiro filho deveria ser um Text, veio {:?}", ast.children[0].kind);
    }
}

#[test]
fn test_kdl_builtin_secure_flag_on_own_continuation_line() {
    // Regressão: `secure` é um flag *de widget* (intrínseco ao TextInput), então
    // é built-in — sem registro do cliente. Numa linha de continuação própria ele
    // dobra no nó acima em vez de virar um nó irmão que engole value/onChange.
    // Era o bug do TextInput `secure` nos templates do rustploy.
    let kdl = r#"
    Column {
        Text "CLIENT SECRET" class="label_cap"
        TextInput ""
            secure
            class="field_input"
            value="gp_client_secret"
            onChange="field:gp_client_secret"
        Text "depois" class="muted"
    }
    "#;
    let ast = glacier_ui::parse_kdl(kdl).unwrap();
    assert_eq!(ast.kind, NodeType::Column);
    // Três filhos: o label, o input e o Text seguinte — sem nó espúrio `secure`.
    assert_eq!(ast.children.len(), 3, "o flag `secure` não pode virar um nó extra");

    let input = &ast.children[1];
    assert_eq!(input.class.as_deref(), Some("field_input"), "a classe deve ficar no input");
    if let NodeType::TextInput { placeholder, value_var, on_change, secure } = &input.kind {
        assert_eq!(placeholder, "", "o placeholder vazio é preservado");
        assert!(*secure, "o flag bare `secure` deve virar secure=true");
        assert_eq!(value_var, "gp_client_secret", "o binding value sobrevive ao fold");
        assert_eq!(on_change, "field:gp_client_secret", "o onChange sobrevive ao fold");
    } else {
        panic!("o segundo filho deveria ser um TextInput, veio {:?}", input.kind);
    }
}

#[test]
fn test_kdl_registered_bare_flag_leading_continuation_with_property() {
    // Um flag bare registrado seguido de propriedades na mesma linha de
    // continuação (`else class="tab_on"`) também deve ser dobrado no nó acima.
    // Re-registrar é idempotente.
    glacier_ui::register_bare_flags(["else"]);
    glacier_ui::register_bare_flags(["else", "secure"]);

    let kdl = r#"
    Row {
        Button "OAuth2"
            else
            class="tab_on"
            onClick="gp_mode:oauth"
    }
    "#;
    let ast = glacier_ui::parse_kdl(kdl).unwrap();
    assert_eq!(ast.children.len(), 1, "o `else` não pode virar um nó irmão");
    let btn = &ast.children[0];
    assert!(btn.is_else, "o flag bare `else` deve marcar is_else");
    assert_eq!(btn.class.as_deref(), Some("tab_on"));
    if let NodeType::Button { text, on_click, .. } = &btn.kind {
        assert_eq!(text, "OAuth2");
        assert_eq!(on_click.as_deref(), Some("gp_mode:oauth"));
    } else {
        panic!("esperava um Button, veio {:?}", btn.kind);
    }
}

#[test]
fn test_kdl_block_else_not_treated_as_flag() {
    // A forma de bloco `} else { ... }` continua sendo um nó-bloco (não um flag
    // de continuação): o flag `else` só dobra quando não abre um bloco.
    let kdl = r#"
    Column {
        if cond="{tab}" equals="git" {
            Text "git"
        } else {
            Text "web"
        }
    }
    "#;
    let ast = glacier_ui::parse_kdl(kdl).unwrap();
    let textos = collect_texts(&ast);
    assert!(textos.contains(&"git".to_string()));
    assert!(textos.contains(&"web".to_string()));
}

#[test]
fn test_kdl_rustploy_remote_ui_templates_parse() {
    // Cobertura de regressão com os templates REAIS da remote-ui do rustploy
    // (snapshots em tests/fixtures/rustploy_remote_ui/). São o motivo original
    // do fix de flags bare em linha de continuação (`secure`, `else`): cada um
    // deve parsear sem que um flag vire um nó irmão espúrio que engole as
    // propriedades seguintes. NB: cópias — se os originais mudarem muito,
    // re-sincronize estes arquivos.
    //
    // `else` é um flag de aplicação, então registramos como a remote-ui faz no
    // boot (`secure`/`password` já são built-in de widget).
    glacier_ui::register_bare_flags(["else", "senao"]);

    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rustploy_remote_ui");
    // Nomes de flag que NUNCA podem aparecer como um nó/componente: indicariam
    // que um flag bare foi lido como início de um novo nó.
    let flag_names = [
        "secure", "password", "seguro", "senha",
        "bold", "negrito", "else", "senao",
        "navigateBack", "navigate_back", "navigate-back", "voltar",
    ];

    for name in ["app", "home", "login", "service", "shell"] {
        let path = format!("{dir}/{name}.kdl");
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("lendo fixture {name}.kdl: {e}"));
        let ast = glacier_ui::parse_kdl(&src)
            .unwrap_or_else(|e| panic!("parse de {name}.kdl falhou: {e}"));

        let mut spurious = Vec::new();
        let mut secure_inputs = 0usize;
        collect_flag_nodes(&ast, &flag_names, &mut spurious, &mut secure_inputs);
        assert!(
            spurious.is_empty(),
            "{name}.kdl: flag bare lido como nó espúrio: {spurious:?}"
        );

        // home.kdl tem dois campos mascarados (CLIENT SECRET e PAT): o flag
        // `secure` em linha própria precisa ter sido dobrado no TextInput.
        if name == "home" {
            assert_eq!(
                secure_inputs, 2,
                "home.kdl deveria ter 2 TextInput secure=true (CLIENT SECRET e PAT)"
            );
        }
    }
}

/// Anda a árvore coletando (1) nós `Component` cujo nome é um keyword de flag
/// bare — sinal de que um flag virou nó por engano — e (2) a contagem de
/// `TextInput` com `secure=true`.
fn collect_flag_nodes(
    node: &UiNode,
    flag_names: &[&str],
    spurious: &mut Vec<String>,
    secure_inputs: &mut usize,
) {
    match &node.kind {
        NodeType::Component { name, .. }
            if flag_names.iter().any(|f| name.eq_ignore_ascii_case(f)) =>
        {
            spurious.push(name.clone());
        }
        NodeType::TextInput { secure: true, .. } => *secure_inputs += 1,
        _ => {}
    }
    for child in &node.children {
        collect_flag_nodes(child, flag_names, spurious, secure_inputs);
    }
}

/// Helper: props de um nó referência de componente (`<NomeComp .../>`).
fn component_props(node: &UiNode) -> std::collections::HashMap<String, String> {
    if let NodeType::Component { props, .. } = &node.kind {
        props.clone()
    } else {
        panic!("esperava um NodeType::Component, veio {:?}", node.kind);
    }
}

// Os cartões inlinados são os Containers de width 320 (assinatura do CartaoKdl).
fn collect_cards(node: &UiNode) -> Vec<&UiNode> {
    let mut out = Vec::new();
    fn walk<'a>(node: &'a UiNode, out: &mut Vec<&'a UiNode>) {
        if matches!(node.kind, NodeType::Container) && node.width.as_deref() == Some("320") {
            out.push(node);
        }
        for child in &node.children {
            walk(child, out);
        }
    }
    walk(node, &mut out);
    out
}

// Coleta o conteúdo de todo Text na árvore avaliada.
fn collect_texts(node: &UiNode) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(node: &UiNode, out: &mut Vec<String>) {
        if let NodeType::Text { content, .. } = &node.kind {
            out.push(content.clone());
        }
        for child in &node.children {
            walk(child, out);
        }
    }
    walk(node, &mut out);
    out
}

#[test]
fn test_unknown_extension_falls_back_to_xml() {
    // Extensão desconhecida (.tmpl) deve usar o parser XML.
    let mut motor = GlacierUI::new();

    std::fs::create_dir_all("templates").ok();
    let path = "templates/test_fallback.tmpl";
    std::fs::write(
        path,
        r##"<Text content="via XML fallback" size="18" />"##,
    ).unwrap();

    motor.register_component("fallback", path).unwrap();

    let ev = motor.evaluated_templates.get("fallback").unwrap();
    if let NodeType::Text { content, .. } = &ev.kind {
        assert_eq!(content, "via XML fallback");
    } else {
        panic!("esperava um Text parseado pelo fallback XML");
    }

    std::fs::remove_file(path).ok();
}
