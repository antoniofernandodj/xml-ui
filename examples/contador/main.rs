use glacier_ui::{Component, Context, GlacierDaemon, Template};

/// Componente que encapsula UI (template XML) + comportamento + estado.
struct Contador {
    valor: i32,
}

impl Contador {
    fn new() -> Self {
        Self { valor: 0 }
    }
}

impl Component for Contador {
    fn name(&self) -> &str {
        "contador"
    }

    fn template(&self) -> Template {
        Template::File("examples/contador/contador.gv".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        ctx.set("contador", self.valor.to_string());
    }

    fn update(&mut self, action: &str, _value: Option<&str>, ctx: &mut Context) {
        match action {
            "incrementar" => self.valor += 1,
            "decrementar" => self.valor -= 1,
            _ => return,
        }
        ctx.set("contador", self.valor.to_string());
    }
}

fn main() -> iced::Result {
    GlacierDaemon::new()
        .title("Glacier - Contador")
        .main(|motor| {
            if let Err(e) = motor.register(Box::new(Contador::new())) {
                eprintln!("Error registering component: {}", e);
            }
            motor.set_initial_screen("contador");
        })
        .run()
}
