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

/// Componente Rust: um contador com botões para abrir janelas novas e um
/// contador de broadcasts recebidos das outras janelas (ver `on_broadcast`).
struct Painel {
    valor: i32,
    recebidos: i32,
}

impl Painel {
    fn new() -> Self {
        Self {
            valor: 0,
            recebidos: 0,
        }
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
        ctx.set("recebidos", self.recebidos.to_string());
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
            // próprio `<script>` Lua), passando de onde veio via `data`.
            "nova_xml" => {
                ctx.open_window(
                    WindowSpec::file("examples/janelas_glacier/detalhe.gv")
                        .title("Detalhe (Lua)")
                        .size(420.0, 320.0)
                        .with_data("origem", "painel"),
                );
            }
            _ => {}
        }
    }

    // Recebe os broadcasts que a janela `detalhe.gv` envia (`avisar_e_fechar`):
    // incrementa o contador e mostra no template. Demonstra o lado RECEPTOR em
    // Rust do IPC entre janelas.
    fn on_broadcast(&mut self, event: &str, payload: &str, ctx: &mut Context) {
        if event == "detalhe_contou" {
            self.recebidos += 1;
            ctx.set("recebidos", self.recebidos.to_string());
            println!("painel recebeu broadcast '{event}': {payload}");
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
