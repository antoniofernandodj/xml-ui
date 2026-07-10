use glacier_ui::{GlacierUI, EngineMessage};
use iced::{Element, Task, widget::text, Color, Subscription};
use std::time::Duration;

/// UI + comportamento no MESMO arquivo: a markup e as funções (`incrementar`/
/// `decrementar`) vivem em `examples/contador_macro/contador_macro.gv`, dentro de `<script>`.
///
/// O `<script>` agora é **Lua**, interpretado em tempo de execução (sem
/// compilar): `register_component` lê o arquivo, carrega o script e roteia cada ação
/// (`on_click`) para a função Lua homônima. As funções leem/escrevem o contexto
/// pela tabela global `ctx`, então `{contador}` na markup reflete `ctx.contador`.
struct App {
    motor: GlacierUI,
}

impl App {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = GlacierUI::new();
        if let Err(e) = motor.register_component("contador", "examples/contador_macro/contador_macro.gv") {
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
        .title("Glacier - Contador (script)")
        .run()
}
