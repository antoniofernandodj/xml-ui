use glacier_ui::{GlacierUI, EngineMessage, Component, Context, Template};
use iced::{Element, Task};
use std::time::Duration;

struct Membro {
    nome: String,
    cargo: String,
    cor: String,
}

/// Cores de avatar usadas em rodízio conforme a lista cresce.
const PALETA: [&str; 5] = ["#89B4FA", "#F5C2E7", "#A6E3A1", "#FAB387", "#CBA6F7"];

/// Membros candidatos adicionados ao clicar no botão.
const CANDIDATOS: [(&str, &str); 4] = [
    ("Marina Costa", "Product Manager"),
    ("Rafael Lima", "Engenheiro de Dados"),
    ("Beatriz Souza", "QA Engineer"),
    ("Diego Alves", "DevOps"),
];

/// Componente que encapsula UI + comportamento de uma lista de membros.
/// Demonstra um sub-componente (`CartaoUsuario`) instanciado num `<ForEach>`;
/// o estado da lista vive no próprio componente e é serializado para o contexto.
struct Lista {
    membros: Vec<Membro>,
    proximo: usize,
}

impl Lista {
    /// Serializa a lista de membros para JSON e publica no contexto.
    /// O `<ForEach items="usuarios">` consome esse array.
    fn sincronizar(&self, ctx: &mut Context) {
        let arr: Vec<serde_json::Value> = self
            .membros
            .iter()
            .map(|m| {
                let inicial = m.nome.chars().next().map(|c| c.to_string()).unwrap_or_default();
                serde_json::json!({
                    "nome": m.nome,
                    "cargo": m.cargo,
                    "inicial": inicial,
                    "cor": m.cor,
                })
            })
            .collect();

        let json = serde_json::Value::Array(arr).to_string();
        ctx.set("usuarios", json);
        ctx.set("total", self.membros.len().to_string());
    }
}

impl Component for Lista {
    fn name(&self) -> &str {
        "lista"
    }

    fn template(&self) -> Template {
        // CartaoUsuario é carregado pelo <import> no topo de lista_usuarios.gv.
        Template::File("examples/lista/lista_usuarios.gv".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        self.sincronizar(ctx);
    }

    fn update(&mut self, action: &str, _value: Option<&str>, ctx: &mut Context) {
        if action == "adicionar" {
            let (nome, cargo) = CANDIDATOS[self.proximo % CANDIDATOS.len()];
            let cor = PALETA[self.membros.len() % PALETA.len()];
            self.membros.push(Membro {
                nome: nome.into(),
                cargo: cargo.into(),
                cor: cor.into(),
            });
            self.proximo += 1;
            self.sincronizar(ctx);
        }
    }
}

struct AppLista {
    motor: GlacierUI,
}

impl AppLista {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = GlacierUI::new();

        let membros = vec![
            Membro { nome: "Clara Silva".into(), cargo: "Engenheira de Software".into(), cor: PALETA[0].into() },
            Membro { nome: "Sophia Martins".into(), cargo: "Designer UI/UX".into(), cor: PALETA[1].into() },
        ];

        if let Err(e) = motor.register(Box::new(Lista { membros, proximo: 0 })) {
            eprintln!("Erro ao registrar 'lista': {}", e);
        }
        motor.set_initial_screen("lista");

        (Self { motor }, Task::none())
    }

    fn update(&mut self, message: EngineMessage) -> Task<EngineMessage> {
        self.motor.dispatch(&message)
    }

    fn view(&self) -> Element<'_, EngineMessage> {
        match self.motor.render_current() {
            Ok(elem) => elem,
            Err(e) => iced::widget::text(format!("Erro ao render: {}", e))
                .color(iced::Color::from_rgb(1.0, 0.0, 0.0))
                .into(),
        }
    }

    fn subscription(&self) -> iced::Subscription<EngineMessage> {
        GlacierUI::reload_subscription(Duration::from_millis(500))
    }
}

fn main() -> iced::Result {
    iced::application(|| AppLista::new(), AppLista::update, AppLista::view)
        .subscription(AppLista::subscription)
        .title("Glacier - Lista de Membros")
        .run()
}
