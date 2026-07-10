use glacier_ui::{Component, Context, GlacierDaemon, Template};

/// Demonstra renderização condicional com `<if>` / `<else>`.
/// O componente encapsula UI + comportamento: o botão alterna `logado`.
struct Condicional;

impl Component for Condicional {
    fn name(&self) -> &str {
        "condicional"
    }

    fn template(&self) -> Template {
        Template::File("examples/condicional/condicional.gv".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        ctx.set("logado", "false");
        ctx.set("usuario", "Clara Silva");
    }

    fn update(&mut self, action: &str, _value: Option<&str>, ctx: &mut Context) {
        match action {
            "login" => ctx.set("logado", "true"),
            "logout" => ctx.set("logado", "false"),
            _ => {}
        }
    }
}

fn main() -> iced::Result {
    GlacierDaemon::new()
        .title("Glacier - Condicional")
        .main(|motor| {
            if let Err(e) = motor.register(Box::new(Condicional)) {
                eprintln!("Erro ao registrar 'condicional': {}", e);
            }
            motor.set_initial_screen("condicional");
        })
        .run()
}
