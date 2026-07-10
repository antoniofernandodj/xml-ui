//! Diálogos modais estilo `QMessageBox` (ver `src/dialogs.rs`): informação,
//! aviso, erro, pergunta e confirmação-com-detalhe, cada um disparado por um
//! botão do painel e fechado por um dos seus próprios botões (ou clicando
//! fora, quando `dismissible`).
//!
//! Rode com: `cargo run --example dialogs`

use glacier_ui::{Component, Context, DialogButton, DialogSpec, GlacierDaemon, Template};

struct Dialogs;

impl Component for Dialogs {
    fn name(&self) -> &str {
        "dialogs"
    }

    fn template(&self) -> Template {
        Template::File("examples/dialogs/dialogs.gv".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        ctx.set("resultado", "(nenhum ainda)");
    }

    fn update(&mut self, action: &str, _value: Option<&str>, ctx: &mut Context) {
        match action {
            "mostrar_info" => ctx.show_dialog(DialogSpec::information(
                "Deploy concluído",
                "O serviço foi publicado e está recebendo tráfego.",
            )),
            "mostrar_warning" => ctx.show_dialog(DialogSpec::warning(
                "Uso de disco alto",
                "O volume de build está com 92% de uso.",
            )),
            "mostrar_error" => ctx.show_dialog(DialogSpec::error(
                "Falha no deploy",
                "Não foi possível iniciar o container: porta 8080 já em uso.",
            )),
            "mostrar_question" => ctx.show_dialog(DialogSpec::question(
                "Reiniciar serviço?",
                "O serviço 'api' está rodando. Deseja reiniciá-lo agora?",
            )),
            "mostrar_confirm" => ctx.show_dialog(
                DialogSpec::confirm("Excluir projeto", "Essa ação não pode ser desfeita.")
                    .with_detail("3 serviços e 2 deployments associados serão removidos.")
                    .with_button(DialogButton::discard("excluir_confirmado")),
            ),
            // Ações despachadas pelos botões dos diálogos acima — o motor já
            // fechou o diálogo antes de rotear aqui.
            "ok" => ctx.set("resultado", "reconhecido (OK)"),
            "yes" => ctx.set("resultado", "serviço reiniciado (Yes)"),
            "no" => ctx.set("resultado", "cancelado (No)"),
            "cancel" => ctx.set("resultado", "cancelado (Cancel)"),
            "excluir_confirmado" => ctx.set("resultado", "projeto excluído (Discard)"),
            _ => {}
        }
    }
}

fn main() -> iced::Result {
    GlacierDaemon::new()
        .title("Glacier - Diálogos")
        .main(|motor| {
            if let Err(e) = motor.register(Box::new(Dialogs)) {
                eprintln!("Error registering component: {}", e);
            }
            motor.set_initial_screen("dialogs");
        })
        .run()
}
