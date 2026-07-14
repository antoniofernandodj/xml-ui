//! Regressão: template XML multi-raiz (par if/else por atributo, sem nó
//! wrapper) deve virar um `Fragment` — em vez de manter
//! silenciosamente só a primeira raiz (o que fazia todo NavItem renderizar o
//! ramo "on" e sumir com o ramo else).

use glacier_ui::GlacierUI;
use glacier_ui::parser::NodeType;

fn write_tmp(name: &str, content: &str) -> String {
    let dir = std::env::temp_dir().join("glacier_repro_navitem");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path.to_string_lossy().to_string()
}

#[test]
fn navitem_if_else_por_atributo_em_componente_multi_raiz() {
    let nav_item = write_tmp(
        "nav_item.gv",
        r#"
<Button class="nav_item_on" if="{view}" equals="{target}" on_click="{action}" text="ON {label}" />
<Button class="nav_item" else on_click="{action}" text="OFF {label}" />
"#,
    );
    let shell = write_tmp(
        "shell.gv",
        &format!(
            r#"
<link rel="import" href="{nav_item}" as="NavItem" />
<Column>
  <NavItem label="Deployments" target="deployments" action="nav_deployments" />
  <NavItem label="Monitoring" target="monitoring" action="nav_monitoring" />
  <NavItem label="Projects" target="projects" action="nav_projects" />
</Column>
"#
        ),
    );

    let mut motor = GlacierUI::new();
    motor.register_component("shell", &shell).unwrap();
    motor.define_data("view", "projects");
    motor.reevaluate_all().unwrap();

    let ev = motor.evaluated("shell").unwrap();
    // A raiz avaliada deve ser a Column com 3 botões (um por NavItem).
    let col = ev;
    assert_eq!(
        col.kind,
        NodeType::Column,
        "raiz deve ser Column, veio {:?}",
        col.kind
    );
    let texts: Vec<String> = col
        .children
        .iter()
        .map(|c| match &c.kind {
            NodeType::Button { text, .. } => text.clone(),
            other => format!("{other:?}"),
        })
        .collect();
    eprintln!("botões renderizados: {texts:?}");
    assert_eq!(
        texts,
        vec!["OFF Deployments", "OFF Monitoring", "ON Projects"],
        "só o Projects (view=projects) deveria estar 'on'"
    );
}
