//! **Múltiplas janelas** com o motor Glacier, sobre o runner [`GlacierDaemon`].
//!
//! Demonstra os dois caminhos de abrir uma janela nova:
//!
//! 1. **Component Rust** — o `update` do componente `Painel` chama
//!    `ctx.open_window_component(Box::new(Painel::new()))`, subindo outra janela
//!    com um `Painel` **independente** (contador próprio, prova de isolamento).
//! 2. **XML via Lua** — o botão "Abrir detalhe" pede uma janela carregando
//!    `detalhe.gv`; esse arquivo tem um `<script>` cujo `open_window(...)` abre,
//!    por sua vez, mais uma janela XML — o caminho Lua puro.
//!
//! Cada janela é um motor Glacier próprio: fechar uma não afeta as outras;
//! fechar a última encerra o app.
//!
//! Rode com: `cargo run --example janelas_glacier`

use glacier_ui::{Component, Context, GlacierDaemon, Template, WindowSpec};

/// Componente Rust: um contador com botões para abrir janelas novas.
struct Painel {
    valor: i32,
}

impl Painel {
    fn new() -> Self {
        Self { valor: 0 }
    }
}

impl Component for Painel {
    fn name(&self) -> &str {
        "painel"
    }

    fn template(&self) -> Template {
        Template::File("examples/janelas_glacier/painel.gv".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        ctx.set("valor", self.valor.to_string());
    }

    fn update(&mut self, action: &str, _value: Option<&str>, ctx: &mut Context) {
        match action {
            "incrementar" => {
                self.valor += 1;
                ctx.set("valor", self.valor.to_string());
            }
            // Caminho Rust: abre outra janela com um `Painel` independente.
            "nova_rust" => {
                ctx.open_window_component(Box::new(Painel::new()));
            }
            // Caminho XML: abre uma janela carregando um template (que tem seu
            // próprio `<script>` Lua).
            "nova_xml" => {
                ctx.open_window(
                    WindowSpec::file("examples/janelas_glacier/detalhe.gv")
                        .title("Detalhe (Lua)")
                        .size(420.0, 320.0),
                );
            }
            _ => {}
        }
    }
}

fn main() -> iced::Result {
    GlacierDaemon::new()
        .title("Glacier - Janela principal")
        .main_size(520.0, 400.0)
        .main(|motor| {
            if let Err(e) = motor.register(Box::new(Painel::new())) {
                eprintln!("Erro ao registrar 'painel': {e}");
            }
            motor.set_initial_screen("painel");
        })
        .run()
}
