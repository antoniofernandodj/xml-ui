//! Formulário estilo Angular Reactive Forms: `FormBuilder` declara os
//! `FormControl`s (nome, valor inicial, validadores), o componente guarda o
//! `Form` construído no seu próprio estado, e o template liga cada
//! `TextInput` a um controle pelo atributo `formControl` — o motor cuida do
//! resto (Enter sempre submete e avança para o próximo campo).
//!
//! Rode com: `cargo run --example formulario_login`

use glacier_ui::{Component, Context, Form, FormBuilder, FormControl, GlacierDaemon, Template};

struct Login {
    form: Form,
}

/// Publica os valores e o primeiro erro de cada controle no contexto — é
/// isso que `formControl`/`{erro_username}` no template consomem. Função
/// livre (não método) para poder ser chamada tanto de dentro do `update`
/// quanto da closure de `on_submit`, que só recebe `&mut Form`.
fn sincronizar(form: &Form, ctx: &mut Context) {
    form.sync_to_context(ctx);
    form.errors_to_context(ctx, "erro_");
}

impl Login {
    fn new() -> Self {
        let form = FormBuilder::new("login")
            .control(FormControl::new("username", "").required().min_length(3))
            .control(FormControl::new("password", "").required().min_length(6))
            // A lógica de submissão fica declarada junto com os controles,
            // em vez de competir com a atualização de campo num único
            // `update()`: veja `Component::on_form_submit` abaixo.
            .on_submit(|form, ctx| {
                if form.is_valid() {
                    ctx.set("status", format!("Bem-vindo, {}!", form.value("username")));
                } else {
                    // Mostra erros também nos campos que o usuário nunca tocou.
                    form.validate();
                    sincronizar(form, ctx);
                    ctx.set("status", "Corrija os campos destacados.");
                }
            })
            .build();

        Self { form }
    }
}

impl Component for Login {
    fn name(&self) -> &str {
        "login"
    }

    fn template(&self) -> Template {
        Template::File("examples/formulario_login/formulario_login.gv".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        ctx.set("status", "");
        sincronizar(&self.form, ctx);
    }

    /// Só lida com atualização de campo: qualquer `TextInput
    /// formControl="x"` dispara a ação "x" a cada tecla; `Form::has_control`
    /// reconhece isso sem precisar de um match por campo. A submissão vai
    /// para `on_form_submit`, não aqui.
    fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context) {
        if self.form.has_control(action) {
            self.form.set_value(action, value.unwrap_or_default());
            sincronizar(&self.form, ctx);
        }
    }

    fn on_form_submit(&mut self, _action: &str, ctx: &mut Context) {
        self.form.submit(ctx);
    }
}

fn main() -> iced::Result {
    GlacierDaemon::new()
        .title("Glacier - Formulário (Reactive Forms)")
        .main(|motor| {
            if let Err(e) = motor.register(Box::new(Login::new())) {
                eprintln!("Erro ao registrar 'login': {}", e);
            }
            motor.set_initial_screen("login");
        })
        .run()
}
