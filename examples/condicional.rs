use xml_ui::{UiEngine, EngineMessage};
use iced::{Element, Task};
use std::time::Duration;

/// Demonstra renderização condicional com `<If>` / `<Else>`.
/// O botão alterna o estado `logado`, e a UI muda de acordo.
struct AppCond {
    motor: UiEngine,
}

impl AppCond {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = UiEngine::new();

        if let Err(e) = motor.register_component("condicional", "templates/condicional.xml") {
            eprintln!("Erro ao registrar 'condicional': {}", e);
        }

        motor.define_data("logado", "false");
        motor.define_data("usuario", "Clara Silva");

        (Self { motor }, Task::none())
    }

    fn update(&mut self, message: EngineMessage) -> Task<EngineMessage> {
        match message {
            EngineMessage::XmlClick(acao) => match acao.as_str() {
                "login" => self.motor.define_data("logado", "true"),
                "logout" => self.motor.define_data("logado", "false"),
                _ => {}
            },
            EngineMessage::FileChanged(_) => {
                let _ = self.motor.check_reload();
            }
            _ => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, EngineMessage> {
        self.motor.render("condicional").unwrap_or_else(|e| {
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
