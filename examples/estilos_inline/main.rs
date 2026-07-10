//! Estilos `.gss` COM ESCOPO escritos INLINE no template, via bloco `<style>`.
//!
//! Diferente de `examples/estilos.rs` (que carrega `.gss` de arquivo, global e
//! por `<link>`), aqui as classes vivem no próprio `examples/estilos_inline/estilos_inline.gv`,
//! dentro de um `<style>`. O corpo é GSS e fica escopado a este componente —
//! nenhum arquivo `.gss` separado. Tudo com hot-reload: edite o `<style>` com a
//! app rodando.
//!
//! Rode com: `cargo run --example estilos_inline`

use glacier_ui::{GlacierUI, EngineMessage, Component, Context, Template};
use iced::{Element, Task};
use std::time::Duration;

struct Estilos {
    valor: i32,
}

impl Component for Estilos {
    fn name(&self) -> &str { "estilos_inline" }

    fn template(&self) -> Template {
        Template::File("examples/estilos_inline/estilos_inline.gv".into())
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

struct App {
    motor: GlacierUI,
}

impl App {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = GlacierUI::new();
        if let Err(e) = motor.register(Box::new(Estilos { valor: 0 })) {
            eprintln!("Erro ao registrar 'estilos_inline': {}", e);
        }
        motor.set_initial_screen("estilos_inline");
        (Self { motor }, Task::none())
    }

    fn update(&mut self, message: EngineMessage) -> Task<EngineMessage> {
        self.motor.dispatch(&message)
    }

    fn view(&self) -> Element<'_, EngineMessage> {
        self.motor.render_current().unwrap_or_else(|e| {
            iced::widget::text(format!("Erro ao renderizar: {}", e))
                .color(iced::Color::from_rgb(1.0, 0.0, 0.0))
                .into()
        })
    }

    fn subscription(&self) -> iced::Subscription<EngineMessage> {
        GlacierUI::reload_subscription(Duration::from_millis(500))
    }

    fn theme(&self) -> iced::Theme {
        self.motor.theme()
    }
}

fn main() -> iced::Result {
    iced::application(|| App::new(), App::update, App::view)
        .subscription(App::subscription)
        .theme(App::theme)
        .title("Glacier - Estilos inline (XML)")
        .run()
}
