//! Formulário estilo Angular Reactive Forms: `FormBuilder` declara os
//! `FormControl`s (nome, valor inicial, validadores), o componente guarda o
//! `Form` construído no seu próprio estado, e o template liga cada
//! `TextInput` a um controle pelo atributo `formControl` — o motor cuida do
//! resto (Enter sempre submete e avança para o próximo campo).
//!
//! Rode com: `cargo run --example formulario_login`

use glacier_ui::{Component, Context, EngineMessage, Form, FormBuilder, FormControl, GlacierUI, Template};
use iced::{widget::text, Color, Element, Subscription, Task};
use std::time::Duration;

struct Login { form: Form }
impl Login {
    fn novo() -> Self {
        Self {
            form: FormBuilder::new("login")
                .control(
                    FormControl::new("username", "")
                        .required()
                        .min_length(3
                    )
                )
                .control(
                    FormControl::new("password", "")
                        .required()
                        .min_length(6)
                    )
                .build(),
        }
    }

    /// Publica os valores e o primeiro erro de cada controle no contexto —
    /// é isso que `formControl`/`{erro_username}` no template consomem.
    fn sincronizar(&self, ctx: &mut Context) {
        self.form.sync_to_context(ctx);
        ctx.set(
            "erro_username", 
            self.form.errors("username").first().cloned().unwrap_or_default()
        );
        ctx.set(
            "erro_password", 
            self.form.errors("password").first().cloned().unwrap_or_default()
        );
    }
}

impl Component for Login {
    fn name(&self) -> &str {
        "login"
    }

    fn template(&self) -> Template {
        Template::File("templates/formulario_login.kdl".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        ctx.set("status", "");
        self.sincronizar(ctx);
    }

    fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context) {
        // Qualquer `TextInput formControl="x"` dispara a ação "x" a cada
        // tecla; `Form::has_control` reconhece isso sem precisar de um match
        // por campo.
        if self.form.has_control(action) {
            self.form.set_value(action, value.unwrap_or_default());
            self.sincronizar(ctx);
            return;
        }

        if action == "entrar" {
            if self.form.is_valid() {
                ctx.set("status", format!("Bem-vindo, {}!", self.form.value("username")));
            } else {
                // Mostra erros também nos campos que o usuário nunca tocou.
                self.form.validate();
                self.sincronizar(ctx);
                ctx.set("status", "Corrija os campos destacados.");
            }
        }
    }
}

struct AppLogin { motor: GlacierUI }
impl AppLogin {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = GlacierUI::new();
        if let Err(e) = motor.register(Box::new(Login::novo())) {
            eprintln!("Erro ao registrar 'login': {}", e);
        }
        motor.set_initial_screen("login");
        (Self { motor }, Task::none())
    }

    fn update(&mut self, message: EngineMessage) -> Task<EngineMessage> {
        self.motor.dispatch(&message)
    }

    fn view(&self) -> Element<'_, EngineMessage> {
        match self.motor.render_current() {
            Ok(elem) => elem,
            Err(e) => text(format!("Erro ao renderizar: {}", e))
                .color(Color::from_rgb(1.0, 0.0, 0.0))
                .into(),
        }
    }

    fn subscription(&self) -> Subscription<EngineMessage> {
        GlacierUI::reload_subscription(Duration::from_millis(500))
    }
}

fn main() -> iced::Result {
    iced::application(|| AppLogin::new(), AppLogin::update, AppLogin::view)
        .subscription(AppLogin::subscription)
        .title("Glacier - Formulário (Reactive Forms)")
        .run()
}
