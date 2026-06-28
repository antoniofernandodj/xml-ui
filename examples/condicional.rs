use xml_ui::{UiEngine, EngineMessage, Component, Context, Template};
use iced::{Element, Task};
use std::time::Duration;

/// Demonstra renderização condicional com `<if>` / `<else>`.
/// O componente encapsula UI + comportamento: o botão alterna `logado`.
struct Condicional;

impl Component for Condicional {
    fn name(&self) -> &str {
        "condicional"
    }

    fn template(&self) -> Template {
        Template::File("templates/condicional.xml".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        ctx.set("logado", "false");
        ctx.set("usuario", "Clara Silva");
    }

    fn update(&mut self, action: &str, _value: Option<&str>, ctx: &mut Context) {
        match action {
            "login" => ctx.set("logado", "true"),
            "logout" => ctx.set("logado", "false"),
            _ => {}
        }
    }
}

struct AppCond {
    motor: UiEngine,
}

impl AppCond {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = UiEngine::new();
        if let Err(e) = motor.register(Box::new(Condicional)) {
            eprintln!("Erro ao registrar 'condicional': {}", e);
        }
        motor.set_initial_screen("condicional");

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
}

fn main() -> iced::Result {
    iced::application("XML UI - Condicional", AppCond::update, AppCond::view)
        .subscription(AppCond::subscription)
        .run_with(|| AppCond::new())
}
