use glacier_ui::{EngineMessage, GlacierUI};
use iced::{widget::text, Color, Element, Subscription, Task};
use std::time::Duration;

/// Imports na camada Lua: o `<script>` de `app.gv` divide a lógica em
/// bibliotecas e as importa com `require`, mantendo tudo encapsulado:
///
/// ```luau
/// local http    = require("net.http_client")  -- net/http_client.luau (client de rede)
/// local strings = require("util.strings")      -- util/strings.luau    (lógica pura)
/// ```
///
/// `require("a.b")` procura `a/b.luau` (e `a/b/init.luau`) relativo ao diretório
/// do template, depois em `<dir>/lib`, depois nos caminhos de `GLACIER_LUA_PATH`.
/// Os módulos rodam no mesmo interpretador do componente, então o client de
/// rede pode usar `fetch` (async/await via corrotina) por baixo dos panos.
struct App {
    motor: GlacierUI,
}

impl App {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = GlacierUI::new();
        if let Err(e) = motor.register_component("app", "examples/imports_luau/app.gv") {
            eprintln!("Erro ao registrar: {}", e);
        }
        motor.set_initial_screen("app");
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
        .title("Glacier - imports em Luau")
        .run()
}
