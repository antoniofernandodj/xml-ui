use glacier_ui::{GlacierUI, EngineMessage, Component, Context, Template};
use iced::{Element, Task, widget::text, Color, Subscription};
use std::time::Duration;

/// Demonstra um `Component` registrado DENTRO de outro `Component`.
///
/// O pai (`Painel`) possui o filho (`CartaoContador`) via `children()`, e cada
/// um trata as ações que saem da sua própria UI:
///   - `+` / `-`  (UI do filho)  -> `CartaoContador::update`
///   - "Trocar tema" (UI do pai) -> `Painel::update`
///
/// O motor faz isso namespaceando as ações da subárvore do filho
/// (`incrementar` -> `CartaoContador::incrementar`) e roteando no `dispatch`.

/// Filho com comportamento e estado próprios.
struct CartaoContador {
    valor: i32,
}

impl Component for CartaoContador {
    fn name(&self) -> &str {
        "CartaoContador"
    }

    fn template(&self) -> Template {
        Template::File("examples/aninhado/cartao_contador.gv".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        ctx.set("valor", self.valor.to_string());
    }

    fn update(&mut self, action: &str, _value: Option<&str>, ctx: &mut Context) {
        match action {
            "incrementar" => self.valor += 1,
            "decrementar" => self.valor -= 1,
            _ => return,
        }
        ctx.set("valor", self.valor.to_string());
    }
}

/// Pai: controla o tema e possui o `CartaoContador`.
struct Painel {
    escuro: bool,
}

impl Painel {
    fn aplicar_tema(&self, ctx: &mut Context) {
        if self.escuro {
            ctx.set("tema", "escuro");
            ctx.set("painel_bg", "#11111B");
            ctx.set("cor_texto", "#F5E0DC");
        } else {
            ctx.set("tema", "claro");
            ctx.set("painel_bg", "#2E3440");
            ctx.set("cor_texto", "#ECEFF4");
        }
    }
}

impl Component for Painel {
    fn name(&self) -> &str {
        "painel"
    }

    fn template(&self) -> Template {
        Template::File("examples/aninhado/painel.gv".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        self.aplicar_tema(ctx);
    }

    fn update(&mut self, action: &str, _value: Option<&str>, ctx: &mut Context) {
        if action == "trocar_tema" {
            self.escuro = !self.escuro;
            self.aplicar_tema(ctx);
        }
    }

    fn children(&self) -> Vec<Box<dyn Component>> {
        vec![Box::new(CartaoContador { valor: 0 })]
    }
}

struct App {
    motor: GlacierUI,
}

impl App {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = GlacierUI::new();
        // Registra só o pai; o filho entra em cascata via children().
        if let Err(e) = motor.register(Box::new(Painel { escuro: false })) {
            eprintln!("Erro ao registrar 'painel': {}", e);
        }
        motor.set_initial_screen("painel");

        (Self { motor }, Task::none())
    }

    fn update(&mut self, message: EngineMessage) -> Task<EngineMessage> {
        self.motor.dispatch(&message)
    }

    fn view(&self) -> Element<'_, EngineMessage> {
        self.motor.render_current().unwrap_or_else(|e| {
            text(format!("Erro ao renderizar: {}", e))
                .color(Color::from_rgb(1.0, 0.0, 0.0))
                .into()
        })
    }

    fn subscription(&self) -> Subscription<EngineMessage> {
        GlacierUI::reload_subscription(Duration::from_millis(500))
    }
}

fn main() -> iced::Result {
    iced::application(|| App::new(), App::update, App::view)
        .subscription(App::subscription)
        .title("Glacier - Componentes Aninhados")
        .run()
}
