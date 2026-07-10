use glacier_ui::{Component, Context, GlacierDaemon, Template};

/// Demonstra navegação entre telas: cada tela é um componente registrado, e os
/// botões declaram o destino no próprio XML via `navigateTo`/`navigateBack` — a
/// navegação é tratada pelo motor em `dispatch`. O estado (`user_name`) é
/// compartilhado entre todas as telas.
///
/// As três telas reaproveitam o mesmo tipo `Tela`, instanciado com nomes e
/// templates diferentes.
struct Tela {
    nome: &'static str,
    template: &'static str,
}

impl Component for Tela {
    fn name(&self) -> &str {
        self.nome
    }

    fn template(&self) -> Template {
        Template::File(self.template.into())
    }

    fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context) {
        if action == "mudar_nome" {
            if let Some(v) = value {
                ctx.set("user_name", v);
            }
        }
    }
}

fn main() -> iced::Result {
    GlacierDaemon::new()
        .title("Glacier - Navegação")
        .main(|motor| {
            let telas: [Tela; 3] = [
                Tela { nome: "home", template: "examples/navegacao/nav_home.gv" },
                Tela { nome: "perfil", template: "examples/navegacao/nav_perfil.gv" },
                Tela { nome: "config", template: "examples/navegacao/nav_config.gv" },
            ];
            for tela in telas {
                let nome = tela.nome;
                if let Err(e) = motor.register(Box::new(tela)) {
                    eprintln!("Erro ao registrar '{}': {}", nome, e);
                }
            }

            // Estado compartilhado entre as telas (não pertence a uma tela só).
            motor.define_data("user_name", "Clara Silva");
            motor.define_data("user_role", "Engenheira de Software");

            // Tela inicial.
            motor.set_initial_screen("home");
        })
        .run()
}
