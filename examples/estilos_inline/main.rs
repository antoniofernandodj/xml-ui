//! Estilos `.gss` COM ESCOPO escritos INLINE no template, via bloco `<style>`.
//!
//! Diferente de `examples/estilos.rs` (que carrega `.gss` de arquivo, global e
//! por `<link>`), aqui as classes vivem no próprio `examples/estilos_inline/estilos_inline.gv`,
//! dentro de um `<style>`. O corpo é GSS e fica escopado a este componente —
//! nenhum arquivo `.gss` separado. Tudo com hot-reload: edite o `<style>` com a
//! app rodando.
//!
//! Rode com: `cargo run --example estilos_inline`

use glacier_ui::{Component, Context, GlacierDaemon, Template};

struct Estilos {
    valor: i32,
}

impl Component for Estilos {
    fn name(&self) -> &str {
        "estilos_inline"
    }

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

fn main() -> iced::Result {
    GlacierDaemon::new()
        .title("Glacier - Estilos inline (XML)")
        .main(|motor| {
            if let Err(e) = motor.register(Box::new(Estilos { valor: 0 })) {
                eprintln!("Erro ao registrar 'estilos_inline': {}", e);
            }
            motor.set_initial_screen("estilos_inline");
        })
        .run()
}
