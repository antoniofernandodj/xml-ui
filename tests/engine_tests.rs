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
            <If cond="{logado}">
                <Text content="Olá, {usuario}" />
            </If>
            <Else>
                <Text content="Entre, por favor" />
            </Else>
            <If cond="{papel}" equals="admin">
                <Text content="painel admin" />
            </If>
        </Column>
        "##
    ).unwrap();

    motor.register_component("cond", path).unwrap();

    // Estado inicial: deslogado, papel comum.
    motor.define_data("logado", "false");
    motor.define_data("usuario", "Ana");
    motor.define_data("papel", "user");

    let ev = motor.evaluated_templates.get("cond").unwrap();
    assert_eq!(ev.children.len(), 1, "só o ramo Else deve aparecer");
    if let NodeType::Text { content, .. } = &ev.children[0].kind {
        assert_eq!(content, "Entre, por favor");
    } else {
        panic!("esperava o Text do Else");
    }

    // Loga como admin: ramo If + comparação equals=admin.
    motor.define_data("logado", "true");
    motor.define_data("papel", "admin");

    let ev = motor.evaluated_templates.get("cond").unwrap();
    assert_eq!(ev.children.len(), 2, "ramo If verdadeiro + bloco admin");
    if let NodeType::Text { content, .. } = &ev.children[0].kind {
        assert_eq!(content, "Olá, Ana");
    } else {
        panic!("esperava o Text do If");
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

