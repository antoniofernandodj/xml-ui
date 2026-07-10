use glacier_ui::{GlacierUI, UiNode, NodeType};

#[test]
fn test_parser_basic() {
    let xml = r##"
    <Container padding="15" width="200" background="#FFFFFF">
        <Column spacing="10">
            <Text content="Hello World" size="20" bold="true" />
            <Button text="Click Me" on_click="btn_click" />
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
    
    let temp_xml_path = "templates/test_temp.gv";
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
    
    let main_path = "templates/test_main.gv";
    let card_path = "templates/test_card.gv";

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
    let path = "templates/test_if.gv";
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

    let main_path = "templates/test_imp_main.gv";
    let card_path = "templates/test_imp_card.gv";
    let badge_path = "templates/test_imp_badge.gv";

    // badge: folha, sem imports.
    std::fs::write(
        badge_path,
        r##"<Text content="[{label}]" />"##
    ).unwrap();

    // card: importa badge e o usa pelo nome.
    std::fs::write(
        card_path,
        r##"<import name="Badge" from="templates/test_imp_badge.gv" />
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
        r##"<import name="Card" from="templates/test_imp_card.gv" />
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

    let main_path = "templates/test_main_comp.gv";
    let card_path = "templates/test_card_comp.gv";

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
fn test_builtin_badge_disponivel_sem_registro() {
    // O app NÃO registra `Badge` — a lib já o registrou sozinha em `new()`.
    // Uma tela pode referenciá-lo por tag e ele resolve, com default e com
    // props sobrescrevendo por instância.
    let mut motor = GlacierUI::new();

    std::fs::create_dir_all("templates").ok();
    let tela_path = "templates/test_builtin_badge.gv";
    std::fs::write(
        tela_path,
        r##"
        <Column>
            <Badge />
            <Badge badge_text="Novo" badge_bg="#A6E3A1" />
        </Column>
        "##,
    )
    .unwrap();

    motor.register_component("tela_badge", tela_path).unwrap();

    let evaluated = motor.evaluated_templates.get("tela_badge").unwrap();
    assert_eq!(evaluated.kind, NodeType::Column);
    assert_eq!(evaluated.children.len(), 2);

    // 1º Badge: sem props -> defaults inline (`{prop|default}`), sem estado global.
    let padrao = &evaluated.children[0];
    assert_eq!(padrao.kind, NodeType::Container);
    assert_eq!(padrao.background.as_deref(), Some("#89B4FA"));
    match &padrao.children[0].kind {
        NodeType::Text { content, color, size, .. } => {
            assert_eq!(content, "Badge");
            assert_eq!(color.as_deref(), Some("#11111B"));
            assert_eq!(*size, Some(13.0)); // default numérico templado
        }
        _ => panic!("Badge padrão deveria conter um Text"),
    }

    // 2º Badge: props sobrescrevem por instância; a omitida (`badge_fg`) mantém o default.
    let custom = &evaluated.children[1];
    assert_eq!(custom.background.as_deref(), Some("#A6E3A1"));
    match &custom.children[0].kind {
        NodeType::Text { content, color, .. } => {
            assert_eq!(content, "Novo");
            assert_eq!(color.as_deref(), Some("#11111B"));
        }
        _ => panic!("Badge custom deveria conter um Text"),
    }

    // O contexto global NÃO foi poluído com defaults (chaves `badge_*`).
    assert!(!motor.context_data.contains_key("badge_text"));
    assert!(!motor.context_data.contains_key("badge_bg"));

    std::fs::remove_file(tela_path).ok();
}

#[test]
fn test_template_default_inline() {
    use glacier_ui::process_template;
    use std::collections::HashMap;

    let mut ctx = HashMap::new();
    ctx.insert("nome".to_string(), "Ana".to_string());

    // Chave presente: usa o valor (o default é ignorado).
    assert_eq!(process_template("Oi {nome|visitante}", &ctx), "Oi Ana");
    // Chave ausente: cai no default.
    assert_eq!(process_template("Oi {cargo|visitante}", &ctx), "Oi visitante");
    // Sem default e ausente: vazio (comportamento antigo, inalterado).
    assert_eq!(process_template("Oi {cargo}", &ctx), "Oi ");
    // Espaços em torno da chave e do default são aparados.
    assert_eq!(process_template("{ cargo | dev }", &ctx), "dev");
}

#[test]
fn test_atributo_numerico_templado() {
    // `size` (numérico) recebe `{prop}` de uma instância de componente e é
    // resolvido no eval — antes só atributos string aceitavam template.
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();

    let card_path = "templates/test_num_card.gv";
    let main_path = "templates/test_num_main.gv";

    std::fs::write(
        card_path,
        r##"<Text content="oi" size="{s}" />"##,
    )
    .unwrap();
    std::fs::write(
        main_path,
        r##"<Column>
            <NumCard s="28" />
            <NumCard />
        </Column>"##,
    )
    .unwrap();

    motor.register_component("NumCard", card_path).unwrap();
    motor.register_component("test_num_main", main_path).unwrap();

    let evaluated = motor.evaluated_templates.get("test_num_main").unwrap();
    // Com prop: size templado resolve para 28.
    match &evaluated.children[0].kind {
        NodeType::Text { size, .. } => assert_eq!(*size, Some(28.0)),
        _ => panic!("esperava Text"),
    }
    // Sem prop: `{s}` resolve vazio -> não parseia -> size fica None.
    match &evaluated.children[1].kind {
        NodeType::Text { size, .. } => assert_eq!(*size, None),
        _ => panic!("esperava Text"),
    }

    std::fs::remove_file(card_path).ok();
    std::fs::remove_file(main_path).ok();
}

#[test]
fn test_foreach_com_componente() {
    let mut motor = GlacierUI::new();

    std::fs::create_dir_all("templates").ok();

    let main_path = "templates/test_lista.gv";
    let card_path = "templates/test_cartao.gv";

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
    
    let path = "templates/test_foreach.gv";
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
        Template::Inline(r#"<Container><Button text="C" on_click="ping" /></Container>"#.into())
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
            r#"<Container><Button text="P" on_click="parent_act" /><ChildComp /></Container>"#.into(),
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

// --- Drag-and-drop list reordering ------------------------------------------

/// A reorderable list (`ForEach ... onReorder="reordered" reorderKey="key"`)
/// with a `dragHandle` on each item's `Text`. Records the final order it's
/// asked to persist.
struct EnvComp;
impl Component for EnvComp {
    fn name(&self) -> &str { "envcomp" }
    fn template(&self) -> Template {
        Template::Inline(r#"
            <Column>
                <ForEach items="rows" var="e" onReorder="reordered" reorderKey="key">
                    <Row>
                        <Text content="{e.key}" dragHandle="true" />
                    </Row>
                </ForEach>
            </Column>
        "#.into())
    }
    fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context) {
        if action == "reordered" {
            ctx.set("last_order", value.unwrap_or_default());
        }
    }
}

#[test]
fn test_drag_reorder_end_to_end() {
    let mut motor = GlacierUI::new();
    motor.register(Box::new(EnvComp)).unwrap();
    motor.set_initial_screen("envcomp");
    motor.define_data("rows", r#"[{"key":"a"},{"key":"b"},{"key":"c"}]"#);

    // Grab "a", drag it over "c" — order live-reflows to [b, c, a].
    let _ = motor.dispatch(&EngineMessage::DragStart {
        list: "rows".into(),
        reorder_key: "key".into(),
        on_reorder: "reordered".into(),
        order: vec!["a".into(), "b".into(), "c".into()],
        key: "a".into(),
    });
    let _ = motor.dispatch(&EngineMessage::DragHover { list: "rows".into(), key: "c".into() });
    assert_eq!(
        motor.get_data("rows").map(String::as_str),
        Some(r#"[{"key":"b"},{"key":"c"},{"key":"a"}]"#),
        "context should reflect the live reflow while still dragging",
    );
    assert_eq!(motor.get_data("last_order"), None, "onReorder only fires on drop");

    // Drop: the component's `update` receives the final order.
    let _ = motor.dispatch(&EngineMessage::DragEnd);
    assert_eq!(motor.get_data("last_order").map(String::as_str), Some(r#"["b","c","a"]"#));

    // A stray release with nothing in progress is a harmless no-op.
    let _ = motor.dispatch(&EngineMessage::DragEnd);
}

#[test]
fn test_drag_hover_ignores_other_lists_and_self() {
    let mut motor = GlacierUI::new();
    motor.register(Box::new(EnvComp)).unwrap();
    motor.set_initial_screen("envcomp");
    motor.define_data("rows", r#"[{"key":"a"},{"key":"b"}]"#);

    let _ = motor.dispatch(&EngineMessage::DragStart {
        list: "rows".into(),
        reorder_key: "key".into(),
        on_reorder: "reordered".into(),
        order: vec!["a".into(), "b".into()],
        key: "a".into(),
    });
    // Hovering a different list, or the dragged item itself, changes nothing.
    let _ = motor.dispatch(&EngineMessage::DragHover { list: "other".into(), key: "b".into() });
    let _ = motor.dispatch(&EngineMessage::DragHover { list: "rows".into(), key: "a".into() });
    assert_eq!(motor.get_data("rows").map(String::as_str), Some(r#"[{"key":"a"},{"key":"b"}]"#));

    let _ = motor.dispatch(&EngineMessage::DragEnd);
    assert_eq!(motor.get_data("last_order").map(String::as_str), Some(r#"["a","b"]"#));
}

#[test]
fn test_gss_fill_and_max_width_resolve_from_class() {
    // `.panel { width: fill; max-width: N }` — the responsive readability-cap
    // pattern (fill up to N, shrink below). Both must land on the node.
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();

    let gss = "templates/test_maxw.gss";
    std::fs::write(gss, ".panel { width: fill; max-width: 640; }").unwrap();

    let path = "templates/test_maxw.gv";
    std::fs::write(path, r##"<Container class="panel" />"##).unwrap();

    motor.load_stylesheet(gss).unwrap();
    motor.register_component("maxw", path).unwrap();

    let n = motor.evaluated_templates.get("maxw").unwrap();
    assert_eq!(n.width.as_deref(), Some("fill"), "width: fill applies from the class");
    assert_eq!(n.max_width, Some(640.0), "max-width applies from the class");

    std::fs::remove_file(gss).ok();
    std::fs::remove_file(path).ok();
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
fn test_link_stylesheet_is_global() {
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();

    let global_gss = "templates/test_global.gss";
    std::fs::write(global_gss, ".box { padding: 5; color: #111111; }").unwrap();

    // Only component A declares this <link>, but it must still reach B: a
    // `<link rel="stylesheet">` is always global, regardless of which
    // template declares it. It overrides `.box`'s padding and adds `.linked`.
    let linked_gss = "templates/test_linked.gss";
    std::fs::write(linked_gss, ".box { padding: 9; } .linked { color: #abcabc; }").unwrap();

    // A links the sheet (as a top-level sibling, before its root, to
    // exercise the <link> hoisting in parse_xml).
    let a_path = "templates/test_scoped_a.gv";
    std::fs::write(
        a_path,
        r##"
        <link rel="stylesheet" href="templates/test_linked.gss" />
        <Text class="box linked" content="A" />
        "##,
    ).unwrap();

    // B doesn't declare the <link> itself, but should see its effect anyway.
    let b_path = "templates/test_scoped_b.gv";
    std::fs::write(b_path, r##"<Text class="box linked" content="B" />"##).unwrap();

    motor.load_stylesheet(global_gss).unwrap();
    motor.register_component("a", a_path).unwrap();
    motor.register_component("b", b_path).unwrap();

    let a = motor.evaluated_templates.get("a").unwrap();
    let b = motor.evaluated_templates.get("b").unwrap();

    assert_eq!(a.padding.as_deref(), Some("9"), "linked class overrides global padding in A");
    assert_eq!(text_color(&a.kind).as_deref(), Some("#abcabc"), "linked class color applies in A");

    // B: the sheet A linked applies here too, since <link rel="stylesheet"> is global.
    assert_eq!(b.padding.as_deref(), Some("9"), "linked sheet reaches B even though only A declared the <link>");
    assert_eq!(text_color(&b.kind).as_deref(), Some("#abcabc"), "linked class color reaches B too");

    std::fs::remove_file(global_gss).ok();
    std::fs::remove_file(linked_gss).ok();
    std::fs::remove_file(a_path).ok();
    std::fs::remove_file(b_path).ok();
}

#[test]
fn test_inline_style_block_default_is_global() {
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();

    let global_gss = "templates/test_istyle_global.gss";
    std::fs::write(global_gss, ".box { padding: 5; color: #111111; }").unwrap();

    // A declares a plain (unscoped) inline <style>, which is global by
    // default — it overrides `.box` and adds `.inlined` for every component.
    let a_path = "templates/test_istyle_a.gv";
    std::fs::write(
        a_path,
        r##"
        <style>
            .box { padding: 9; }
            .inlined { color: #abcabc; }
        </style>
        <Text class="box inlined" content="A" />
        "##,
    ).unwrap();

    // B declares nothing, but should see A's plain <style> anyway.
    let b_path = "templates/test_istyle_b.gv";
    std::fs::write(b_path, r##"<Text class="box inlined" content="B" />"##).unwrap();

    motor.load_stylesheet(global_gss).unwrap();
    motor.register_component("a", a_path).unwrap();
    motor.register_component("b", b_path).unwrap();

    let a = motor.evaluated_templates.get("a").unwrap();
    let b = motor.evaluated_templates.get("b").unwrap();

    assert_eq!(a.padding.as_deref(), Some("9"), "inline <style> overrides global padding");
    assert_eq!(text_color(&a.kind).as_deref(), Some("#abcabc"), "inline class color applies in A");

    // B: A's plain inline <style> reaches it too, since it's global by default.
    assert_eq!(b.padding.as_deref(), Some("9"), "B sees A's unscoped inline <style> too");
    assert_eq!(text_color(&b.kind).as_deref(), Some("#abcabc"), "B sees A's unscoped inline class color too");

    std::fs::remove_file(global_gss).ok();
    std::fs::remove_file(a_path).ok();
    std::fs::remove_file(b_path).ok();
}

#[test]
fn test_inline_style_block_scoped_true_is_scoped() {
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();

    // Global sheet seen by everyone.
    let global_gss = "templates/test_istyle_scoped_global.gss";
    std::fs::write(global_gss, ".box { padding: 5; color: #111111; }").unwrap();

    // A declares an inline <style scoped="true">, which overrides `.box` and
    // adds `.scoped` only within A's own subtree.
    let a_path = "templates/test_istyle_scoped_a.gv";
    std::fs::write(
        a_path,
        r##"
        <style scoped="true">
            .box { padding: 9; }
            .scoped { color: #abcabc; }
        </style>
        <Text class="box scoped" content="A" />
        "##,
    ).unwrap();

    // B declares nothing: it only sees the global sheet.
    let b_path = "templates/test_istyle_scoped_b.gv";
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
    std::fs::remove_file(a_path).ok();
    std::fs::remove_file(b_path).ok();
}

#[test]
fn test_inline_style_overrides_linked_by_document_order() {
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();

    // A linked sheet sets the color; a later inline <style> overrides it —
    // both are global, but a component's own <link>s are installed before its
    // own inline blocks, so document order still determines who wins.
    let linked = "templates/test_istyle_order.gss";
    std::fs::write(linked, ".tag { color: #aaaaaa; padding: 3; }").unwrap();

    let path = "templates/test_istyle_order.gv";
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
fn test_inline_style_reloads_with_template() {
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();

    let path = "templates/test_istyle_reload.gv";
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

    let path = "templates/test_inline.gv";
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

    let child = "templates/test_li_child.gv";
    std::fs::write(child, r##"<Text content="child:{x}" />"##).unwrap();

    let parent = "templates/test_li_parent.gv";
    // Declarative import via <link>; the component is then referenced by name.
    std::fs::write(
        parent,
        r##"
        <link rel="import" href="templates/test_li_child.gv" as="ChildLink" />
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
    let tpl = "templates/test_textarea.gv";
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
    let tpl = "templates/test_select.gv";
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

    let tpl = "templates/test_ifforeach.gv";
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

    let tpl = "templates/test_data.gv";
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

    let tpl = "templates/test_theme.gv";
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
    let path = "templates/test_directives_attr.gv";
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
    let path = "templates/test_precedence.gv";
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

// --- Formulários (`<Form>` / `formControl`) ---------------------------------

/// Coleta, em ordem de documento, cada nó com `formControl` definido: o nome
/// do controle e o próprio nó (já avaliado/hidratado), clonado para escapar do
/// empréstimo da árvore.
fn collect_form_inputs(node: &UiNode, out: &mut Vec<(String, UiNode)>) {
    if let Some(name) = &node.form_control {
        out.push((name.clone(), node.clone()));
    }
    for child in &node.children {
        collect_form_inputs(child, out);
    }
}

/// Componente com um `<Form>` de dois campos, usado pelos testes de
/// hidratação e de dispatch abaixo.
struct FormTestComp;
impl Component for FormTestComp {
    fn name(&self) -> &str { "formtest" }
    fn template(&self) -> Template {
        Template::Inline(r#"
            <Form onSubmit="enviar">
                <TextInput formControl="usuario" />
                <TextInput formControl="senha" secure="true" />
            </Form>
        "#.into())
    }
    fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context) {
        match action {
            "usuario" => { ctx.set("usuario", value.unwrap_or_default()); }
            "senha" => { ctx.set("senha", value.unwrap_or_default()); }
            _ => {}
        }
    }
    // `onSubmit` ("enviar") is routed to `on_form_submit`, not `update` — see
    // `test_ui_submit_always_dispatches_regardless_of_next_focus` below.
    fn on_form_submit(&mut self, _action: &str, ctx: &mut Context) {
        ctx.set("enviado", "true");
    }
}

#[test]
fn test_form_hydrates_scope_submit_and_next_focus() {
    let mut motor = GlacierUI::new();
    motor.register(Box::new(FormTestComp)).unwrap();
    motor.set_initial_screen("formtest");

    let evaluated = motor.evaluated_templates.get("formtest").unwrap();
    let mut inputs = Vec::new();
    collect_form_inputs(evaluated, &mut inputs);
    assert_eq!(inputs.len(), 2, "esperava 2 inputs ligados a formControl, veio {:?}",
        inputs.iter().map(|(n, _)| n).collect::<Vec<_>>());

    let (usuario_name, usuario) = &inputs[0];
    let (senha_name, senha) = &inputs[1];
    assert_eq!(usuario_name, "usuario");
    assert_eq!(senha_name, "senha");

    // O `onSubmit` do `<Form>` chega em todo controle, para o Enter sempre
    // disparar a submissão — independente de qual campo está com foco.
    assert_eq!(usuario.form_submit_action.as_deref(), Some("enviar"));
    assert_eq!(senha.form_submit_action.as_deref(), Some("enviar"));

    // Mesmo `scope` (prefixo do id de foco) em ambos, por pertencerem ao
    // mesmo `<Form>`.
    assert!(usuario.form_scope.is_some());
    assert_eq!(usuario.form_scope, senha.form_scope);

    // Enter em "usuario" também avança o foco para "senha"; em "senha" (o
    // último campo) não há próximo.
    assert_eq!(usuario.form_next_focus.as_deref(), Some("senha"));
    assert_eq!(senha.form_next_focus, None);
}

#[test]
fn test_form_control_input_dispatches_like_on_change() {
    let mut motor = GlacierUI::new();
    motor.register(Box::new(FormTestComp)).unwrap();
    motor.set_initial_screen("formtest");

    // `TextInput formControl="usuario"` sem `onChange` explícito usa o nome
    // do controle como ação — o mesmo canal que um `onChange` manual usaria.
    let _ = motor.dispatch(&EngineMessage::UiInputChanged { action: "usuario".into(), value: "ana".into() });
    assert_eq!(motor.get_data("usuario").map(String::as_str), Some("ana"));
}

#[test]
fn test_ui_submit_always_dispatches_regardless_of_next_focus() {
    // Enter num campo com próximo: dispara `onSubmit` e pede foco adiante.
    let mut motor = GlacierUI::new();
    motor.register(Box::new(FormTestComp)).unwrap();
    motor.set_initial_screen("formtest");
    let _ = motor.dispatch(&EngineMessage::UiSubmit {
        action: "enviar".into(),
        next_focus: Some("glacier_form::formtest::senha".into()),
    });
    assert_eq!(motor.get_data("enviado").map(String::as_str), Some("true"));

    // Enter no último campo (sem próximo): ainda assim dispara `onSubmit` — a
    // decisão de aceitar ou não fica com o `on_form_submit` do componente (via
    // `Form::is_valid()`), não com o motor.
    let mut motor2 = GlacierUI::new();
    motor2.register(Box::new(FormTestComp)).unwrap();
    motor2.set_initial_screen("formtest");
    let _ = motor2.dispatch(&EngineMessage::UiSubmit { action: "enviar".into(), next_focus: None });
    assert_eq!(motor2.get_data("enviado").map(String::as_str), Some("true"));
}

#[test]
fn test_form_control_defaults_value_and_on_change() {
    let xml = r#"
        <Form onSubmit="entrar">
            <TextInput formControl="usuario" />
        </Form>
    "#;
    let ast = UiNode::parse_xml(xml).unwrap();
    match &ast.kind {
        NodeType::Form { on_submit, .. } => assert_eq!(on_submit.as_deref(), Some("entrar")),
        other => panic!("esperava NodeType::Form, veio {:?}", other),
    }

    let input = &ast.children[0];
    assert_eq!(input.form_control.as_deref(), Some("usuario"));
    match &input.kind {
        NodeType::TextInput { value_var, on_change, .. } => {
            assert_eq!(value_var, "usuario");
            assert_eq!(on_change, "usuario");
        }
        other => panic!("esperava NodeType::TextInput, veio {:?}", other),
    }
}

#[test]
fn test_form_control_respects_explicit_value_and_on_change() {
    let xml = r#"
        <Form>
            <TextInput formControl="usuario" value="outro_valor" onChange="outraAcao" />
        </Form>
    "#;
    let ast = UiNode::parse_xml(xml).unwrap();
    let input = &ast.children[0];
    match &input.kind {
        NodeType::TextInput { value_var, on_change, .. } => {
            assert_eq!(value_var, "outro_valor");
            assert_eq!(on_change, "outraAcao");
        }
        other => panic!("esperava NodeType::TextInput, veio {:?}", other),
    }
}

/// Sanity check on the actual shipped template (`examples/formulario_login.rs`
/// uses this same path): parses and evaluates end-to-end and has the two
/// expected `formControl`-bound inputs in order. Loading the real file keeps a
/// broken example template from slipping through `cargo test`.
#[test]
fn test_formulario_login_example_template_parses_and_evaluates() {
    let mut motor = GlacierUI::new();
    motor
        .register_component("formulario_login_smoke", "examples/formulario_login/formulario_login.gv")
        .expect("o template do exemplo formulario_login deve parsear e avaliar sem erro");

    let evaluated = motor.evaluated_templates.get("formulario_login_smoke").unwrap();
    let mut inputs = Vec::new();
    collect_form_inputs(evaluated, &mut inputs);
    assert_eq!(
        inputs.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
        vec!["username".to_string(), "password".to_string()],
    );
    assert_eq!(inputs[0].1.form_next_focus.as_deref(), Some("password"));
    assert_eq!(inputs[1].1.form_next_focus, None);
}

// ── Fragment (multi-root component templates) ───────────────────────────────

/// A component whose template is a fragment (an `if`/`else` pair) splices the
/// matching branch into the parent — no wrapper node, and the branch is chosen
/// per-instance from the passed prop.
#[test]
fn test_fragment_component_splices_if_else_branch() {
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();
    let card = "templates/test_frag_card.gv";
    let main = "templates/test_frag_main.gv";
    std::fs::write(
        card,
        r#"
        <Column if="{filler}" equals="1" class="filler" />
        <Column else class="card">
          <Text content="{name}" />
        </Column>
        "#,
    )
    .unwrap();
    std::fs::write(
        main,
        r#"
        <import name="FragCard" from="templates/test_frag_card.gv" />
        <Column class="grid">
          <FragCard filler="0" name="Alice" />
          <FragCard filler="1" name="Zzz" />
        </Column>
        "#,
    )
    .unwrap();

    motor.register_component("test_frag_main", main).unwrap();
    let evaluated = motor.evaluated_templates.get("test_frag_main").unwrap();

    assert_eq!(evaluated.kind, NodeType::Column);
    // Two spliced siblings — neither is a Fragment wrapper.
    assert_eq!(evaluated.children.len(), 2, "fragment children should be spliced, not wrapped");
    assert!(evaluated.children.iter().all(|c| c.kind != NodeType::Fragment));

    // `class` is resolved into style fields (and cleared) during evaluation,
    // so branches are identified by their structure instead: the `else` card
    // branch has the name `Text`; the `if` filler branch is empty.
    //
    // First instance (filler="0") → the `else` card branch, carrying the name.
    let first = &evaluated.children[0];
    assert_eq!(first.children.len(), 1, "card branch has one child (the name Text)");
    if let NodeType::Text { content, .. } = &first.children[0].kind {
        assert_eq!(content, "Alice");
    } else {
        panic!("card branch should contain the name Text");
    }
    // Second instance (filler="1") → the empty `if` filler branch.
    assert!(evaluated.children[1].children.is_empty(), "filler branch is empty");

    std::fs::remove_file(card).ok();
    std::fs::remove_file(main).ok();
}

// ── Registro unificado: register_component liga Luau se houver <script> ──────

/// `register_component` presume que "sempre pode haver Luau": um template com um
/// bloco `<script>` tem seu comportamento Luau ligado automaticamente (sem um
/// `register_luau` à parte). O `init()` semeia o estado e a ação roteia para a
/// função Luau de mesmo nome.
#[test]
fn test_register_component_wires_luau_when_script_present() {
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();
    let path = "templates/test_scripted_unified.gv";
    std::fs::write(
        path,
        r#"
<Container>
  <Text content="{n}" />
  <Button text="+" onClick="inc" />
</Container>
<script>
function init() ctx.n = ctx.n or 0 end
function inc() ctx.n = ctx.n + 1 end
</script>
"#,
    )
    .unwrap();

    motor.register_component("scripted", path).unwrap();
    motor.set_initial_screen("scripted");

    // init() do <script> semeou o estado.
    assert_eq!(motor.context_data.get("n").map(String::as_str), Some("0"));

    // A ação "inc" roteia para a função Luau homônima do componente scriptado.
    let _ = motor.dispatch(&glacier_ui::EngineMessage::UiClick("inc".into()));
    assert_eq!(motor.context_data.get("n").map(String::as_str), Some("1"));

    std::fs::remove_file(path).ok();
}

/// Sem `<script>`, `register_component` continua só-UI: nenhuma behavior é
/// registrada, então uma ação sem dono simplesmente não faz nada (não entra em
/// pânico) — o mesmo que antes da unificação.
#[test]
fn test_register_component_ui_only_when_no_script() {
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();
    let path = "templates/test_uionly_unified.gv";
    std::fs::write(
        path,
        r#"<Container><Button text="x" onClick="nada" /></Container>"#,
    )
    .unwrap();

    motor.register_component("uionly", path).unwrap();
    motor.set_initial_screen("uionly");
    // Ação sem behavior é no-op (não deve entrar em pânico).
    let _ = motor.dispatch(&glacier_ui::EngineMessage::UiClick("nada".into()));

    std::fs::remove_file(path).ok();
}

#[test]
fn test_text_child_content() {
    // Child text is accepted and normalized (trim + collapse whitespace).
    let xml = "<Text>  lorem   ipsum \n  dolor  </Text>";
    let ast = UiNode::parse_xml(xml).unwrap();
    if let NodeType::Text { content, .. } = &ast.kind {
        assert_eq!(content, "lorem ipsum dolor");
    } else {
        panic!("Root should be Text");
    }
}

#[test]
fn test_text_child_wins_over_attribute() {
    // When both are given, the child takes precedence.
    let xml = r#"<Text content="from attr">from child</Text>"#;
    let ast = UiNode::parse_xml(xml).unwrap();
    if let NodeType::Text { content, .. } = &ast.kind {
        assert_eq!(content, "from child");
    } else {
        panic!("Root should be Text");
    }
}

#[test]
fn test_text_attribute_fallback_when_no_child() {
    let xml = r#"<Text content="only attr" />"#;
    let ast = UiNode::parse_xml(xml).unwrap();
    if let NodeType::Text { content, .. } = &ast.kind {
        assert_eq!(content, "only attr");
    } else {
        panic!("Root should be Text");
    }
}
