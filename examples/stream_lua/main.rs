use glacier_ui::{EngineMessage, GlacierUI};
use iced::{widget::text, Color, Element, Subscription, Task};
use std::time::Duration;

/// Streams de vida longa (SSE e WebSocket) a partir do `<script>` Lua.
///
/// No template `stream_luau.xml`:
/// ```luau
/// sse_conn = sse("https://sse.dev/test", { on_message = "sse_recebeu" })
/// ws_conn  = websocket("wss://echo.websocket.org", { on_message = "ws_recebeu" })
/// ws_conn:send("ping")   -- envia pela conexão viva
/// ```
/// Ao contrário do `fetch` (one-shot, que suspende a corrotina), `sse` e
/// `websocket` NÃO bloqueiam: registram o stream e retornam um handle na hora.
/// Cada evento que chega da rede chama de volta o handler nomeado em `opts`
/// (`on_message`, `on_open`, `on_error`, `on_close`), que escreve em `ctx` e a
/// UI reavalia — como qualquer ação.
///
/// **Importante**: os streams viram `iced::Subscription`s produzidas por
/// [`GlacierUI::subscription`]. Por isso o `subscription()` do app precisa
/// incluir `self.motor.subscription()` (ver abaixo) — sem isso, nenhuma
/// conexão é aberta.
struct App {
    motor: GlacierUI,
}

impl App {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = GlacierUI::new();
        // Por padrão usa os endpoints públicos; aponte para a API local com
        //   GLACIER_STREAM_TEMPLATE=examples/stream_luau/stream_local.xml
        let template = std::env::var("GLACIER_STREAM_TEMPLATE")
            .unwrap_or_else(|_| "examples/stream_luau/stream_luau.xml".to_string());
        if let Err(e) = motor.register_component("stream", &template) {
            eprintln!("Erro ao registrar: {}", e);
        }
        motor.set_initial_screen("stream");

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
        // `motor.subscription()` carrega os streams (SSE/WebSocket) abertos pelo
        // Lua; sem ele, os eventos nunca chegam. O hot-reload é opcional.
        Subscription::batch([
            self.motor.subscription(),
            GlacierUI::reload_subscription(Duration::from_millis(500)),
        ])
    }
}

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .subscription(App::subscription)
        .title("Glacier - SSE + WebSocket em Lua")
        .run()
}
