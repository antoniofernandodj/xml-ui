use glacier_ui::{GlacierUI, EngineMessage};
use iced::{Element, Task, widget::text, Color, Subscription};
use std::time::Duration;

/// Chamadas de rede a partir do `<script>` Lua, sem bloquear a UI.
///
/// No template `fetch_luau.xml`, a função `buscar` faz:
/// ```luau
/// local res = fetch("https://api.ipify.org?format=json")
/// ```
/// `fetch` **suspende a corrotina** da ação (via `coroutine.yield`); o motor
/// dispara a requisição HTTP no executor async do iced (hyper + rustls) e, ao
/// receber a resposta, **retoma a corrotina** no ponto do `fetch` com a tabela
/// `{ ok, status, body, error }`. Do lado do Lua, parece `await` — mas a thread
/// de UI nunca trava (o "carregando..." aparece enquanto a rede responde).
struct App {
    motor: GlacierUI,
}

impl App {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = GlacierUI::new();
        if let Err(e) = motor.register_component("fetch", "examples/fetch_luau/fetch_luau.xml") {
            eprintln!("Erro ao registrar: {}", e);
        }
        motor.set_initial_screen("fetch");

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
        .title("Glacier - fetch em Lua")
        .run()
}
