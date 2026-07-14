use glacier_ui::{Component, Context, ContextVar, GlacierDaemon, Template};

/// Componente que encapsula UI + comportamento de um cartão de perfil editável.
/// Mantém seu próprio estado (`seguindo`) e reage a inputs e cliques.
struct Perfil {
    seguindo: bool,
}

impl Perfil {
    fn new() -> Self {
        Self { seguindo: false }
    }
}

impl Component for Perfil {
    fn name(&self) -> &str {
        "perfil"
    }

    fn template(&self) -> Template {
        // Só o componente de entrada precisa ser declarado; PerfilCard é puxado
        // pelo <import> no topo de perfil.gv.
        Template::File("examples/perfil/perfil.gv".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        let user_name = ContextVar::new("user_name", "Clara Silva");
        let user_role = ContextVar::new("user_role", "Engenheira de Software Senior");
        let texto_botao = ContextVar::new("texto_botao", "Seguir");
        let btn_color = ContextVar::new("btn_color", "#313244"); // Sleek base button color

        ctx.set_var(&user_name);
        ctx.set_var(&user_role);
        ctx.set_var(&texto_botao);
        ctx.set_var(&btn_color);
    }

    fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context) {
        match action {
            "mudar_nome" => {
                if let Some(v) = value {
                    ctx.set("user_name", v);
                }
            }
            "mudar_cargo" => {
                if let Some(v) = value {
                    ctx.set("user_role", v);
                }
            }
            "seguir_usuario" => {
                self.seguindo = !self.seguindo;
                if self.seguindo {
                    ctx.set("texto_botao", "Seguindo ✓");
                    ctx.set("btn_color", "#A6E3A1"); // Light green for active/following
                } else {
                    ctx.set("texto_botao", "Seguir");
                    ctx.set("btn_color", "#313244"); // Back to default dark
                }
            }
            "set_dev" => {
                ctx.set("user_name", "Clara Silva");
                ctx.set("user_role", "Engenheira de Software Senior");
            }
            "set_designer" => {
                ctx.set("user_name", "Sophia Martins");
                ctx.set("user_role", "Designer de Interface (UI/UX)");
            }
            other => println!("Action clicked: {}", other),
        }
    }
}

fn main() -> iced::Result {
    GlacierDaemon::new()
        .title("Glacier - Painel de Perfil")
        .main(|motor| {
            if let Err(e) = motor.register(Box::new(Perfil::new())) {
                eprintln!("Error registering component 'perfil': {}", e);
            }
            motor.set_initial_screen("perfil");
        })
        .run()
}
