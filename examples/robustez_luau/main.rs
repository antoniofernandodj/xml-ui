//! Demonstra as lacunas fechadas em `PLANO_LUAU_ROBUSTEZ.md`: temporizadores
//! (`after`/`cancel`), persistência local (`storage`), leitura de viewport,
//! tabelas aceitas em `ctx` (serializadas via `json.encode`) e erros de
//! script visíveis ao usuário (`on_error`, com fallback automático em toast).
//!
//! Toda a lógica está em `robustez.luau` — este `main.rs` só registra o
//! componente e liga as subscriptions (hot-reload + expiração de toasts).
//!
//! Rode com: `cargo run --example robustez_luau`

use glacier_ui::{EngineMessage, GlacierUI};
use iced::{widget::text, Color, Element, Subscription, Task};
use std::time::Duration;

struct App {
    motor: GlacierUI,
}

impl App {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = GlacierUI::new();
        if let Err(e) =
            motor.register_component("robustez", "examples/robustez_luau/robustez.gv")
        {
            eprintln!("Erro ao registrar: {}", e);
        }
        motor.set_initial_screen("robustez");

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
        Subscription::batch([
            GlacierUI::reload_subscription(Duration::from_millis(500)),
            GlacierUI::toast_subscription(Duration::from_millis(250)),
        ])
    }
}

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .subscription(App::subscription)
        .title("Glacier - robustez da camada Luau")
        .run()
}
