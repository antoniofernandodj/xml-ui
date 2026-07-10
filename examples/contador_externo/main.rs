use glacier_ui::{GlacierUI, EngineMessage};
use iced::{Element, Task, widget::text, Color, Subscription};
use std::time::Duration;

/// Igual ao `contador_macro`, mas o `<script>` **aponta para um arquivo Lua
/// externo** em vez de embutir o código:
///
/// ```xml
/// <script src="contador_externo.luau"></script>
/// ```
///
/// O caminho é resolvido relativo ao diretório do template. `register_component` lê o
/// template, segue o `src`, carrega o Lua e roteia as ações (`on_click`/`onChange`)
/// para as funções homônimas — que leem/escrevem o contexto pela tabela `ctx`.
struct App {
    motor: GlacierUI,
}

impl App {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = GlacierUI::new();
        if let Err(e) = motor.register_component("contador", "examples/contador_externo/contador_externo.gv") {
            eprintln!("Erro ao registrar: {}", e);
        }
        motor.set_initial_screen("contador");

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
        .title("Glacier - Contador (script Lua externo)")
        .run()
}
