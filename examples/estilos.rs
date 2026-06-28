use xml_ui::{UiEngine, EngineMessage, Component, Context, Template};
use iced::{Element, Task};
use std::time::Duration;

/// Demonstra stylesheets `.iss` e `<link>` no template:
/// - GLOBAL: `styles/app.iss`, carregada via `motor.load_stylesheet(...)`,
///   vale para todos os componentes (card, title, subtitle, stack, actions).
/// - ESCOPADA: `<link rel="stylesheet" href="styles/estilos.iss">`, válida só
///   neste componente (as classes `.btn*`).
/// - TEMA: `<link rel="theme" href="styles/theme.json">` define a paleta do
///   `iced` (lido por `motor.theme()` e ligado em `.theme(...)`).
///
/// (`<link>` também aceita `rel="import"`/`"component"` e `rel="data"`.)
/// Atributos inline no nó sempre vencem a classe. Todos os arquivos têm
/// hot-reload: edite-os com a app rodando.
struct Estilos;

impl Component for Estilos {
    fn name(&self) -> &str { "estilos" }

    fn template(&self) -> Template {
        Template::File("templates/estilos.xml".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        ctx.set("valor", "0");
    }

    fn update(&mut self, action: &str, _value: Option<&str>, ctx: &mut Context) {
        let atual: i32 = ctx.get("valor")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
        match action {
            "incrementar" => ctx.set("valor", (atual + 1).to_string()),
            "decrementar" => ctx.set("valor", (atual - 1).to_string()),
            _ => {}
        }
    }
}

struct AppEstilos {
    motor: UiEngine,
}

impl AppEstilos {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = UiEngine::new();
        if let Err(e) = motor.register(Box::new(Estilos)) {
            eprintln!("Erro ao registrar 'estilos': {}", e);
        }
        // Carrega a stylesheet depois do componente: `load_stylesheet`
        // re-avalia todos os templates já registrados com as classes.
        if let Err(e) = motor.load_stylesheet("styles/app.iss") {
            eprintln!("Erro ao carregar stylesheet: {}", e);
        }
        motor.set_initial_screen("estilos");

        (Self { motor }, Task::none())
    }

    fn update(&mut self, message: EngineMessage) -> Task<EngineMessage> {
        if let Err(e) = self.motor.dispatch(&message) {
            eprintln!("Erro no dispatch: {}", e);
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, EngineMessage> {
        self.motor.render_current().unwrap_or_else(|e| {
            iced::widget::text(format!("Erro ao renderizar: {}", e))
                .color(iced::Color::from_rgb(1.0, 0.0, 0.0))
                .into()
        })
    }

    fn subscription(&self) -> iced::Subscription<EngineMessage> {
        UiEngine::reload_subscription(Duration::from_millis(500))
    }

    /// Tema da janela: o que foi carregado via `<link rel="theme">`, ou Dark.
    fn theme(&self) -> iced::Theme {
        self.motor.theme()
    }
}

fn main() -> iced::Result {
    iced::application("XML UI - Estilos (.iss)", AppEstilos::update, AppEstilos::view)
        .subscription(AppEstilos::subscription)
        .theme(AppEstilos::theme)
        .run_with(|| AppEstilos::new())
}
