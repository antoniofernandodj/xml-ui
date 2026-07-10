//! Navegação decidida pelo `<script>` Lua (`navigate`/`navigate_back`), ao
//! invés dos atributos declarativos `navigateTo`/`navigateBack` (ver o
//! exemplo `navegacao`, que só troca de tela).
//!
//! Aqui o clique em "Entrar" só navega se a validação em Lua passar — o
//! próprio botão não sabe (nem pode saber, sendo declarativo) para onde vai.
//!
//! Rode com: `cargo run --example navegacao_luau`

use glacier_ui::{EngineMessage, GlacierUI, GlacierApp};
use iced::{widget::text, Color, Element, Subscription, Task};
use std::time::Duration;

struct App { motor: GlacierUI }
impl GlacierApp for App {
    type Message = EngineMessage;
    fn init() -> (Self, Task<EngineMessage>) {
        let mut motor = GlacierUI::new();
        if let Err(e) = motor
            .register_component("login_luau", "examples/navegacao_luau/login.gv") {
                eprintln!("Erro ao registrar 'login_luau': {}", e);
            }

        if let Err(e) = motor
            .register_component("dashboard_luau", "examples/navegacao_luau/dashboard.gv") {
                eprintln!("Erro ao registrar 'dashboard_luau': {}", e);
            }

        motor.set_initial_screen("login_luau");

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
    App::bootstrap()
        .title("Glacier - navegação via script Lua")
        .run()
}
