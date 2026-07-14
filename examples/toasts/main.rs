//! Notificações toast (ver `src/toasts.rs`): info, sucesso, aviso e erro,
//! empilhadas no canto inferior direito e dispensadas sozinhas depois de
//! alguns segundos (ou clicando no "×" de cada uma).
//!
//! Rode com: `cargo run --example toasts`

use glacier_ui::{Component, Context, GlacierDaemon, Template, ToastSpec};
use std::time::Duration;

struct Toasts {
    disparados: u32,
}

impl Component for Toasts {
    fn name(&self) -> &str {
        "toasts"
    }

    fn template(&self) -> Template {
        Template::File("examples/toasts/toasts.gv".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        ctx.set("total_disparados", "0");
    }

    fn update(&mut self, action: &str, _value: Option<&str>, ctx: &mut Context) {
        match action {
            "mostrar_info" => ctx.show_toast(ToastSpec::info("O deploy foi enfileirado.")),
            "mostrar_sucesso" => {
                ctx.show_toast(ToastSpec::success("Serviço publicado e recebendo tráfego."))
            }
            "mostrar_aviso" => {
                ctx.show_toast(ToastSpec::warning("O volume de build está com 92% de uso."))
            }
            "mostrar_erro" => ctx.show_toast(ToastSpec::error(
                "Não foi possível iniciar o container: porta 8080 já em uso.",
            )),
            "mostrar_com_titulo" => ctx.show_toast(
                ToastSpec::warning("Esse toast fica em exibição por 10 segundos.")
                    .with_title("Duração customizada")
                    .with_duration(Duration::from_secs(10)),
            ),
            "mostrar_varios" => {
                ctx.show_toast(ToastSpec::info("1 de 3: clonando repositório..."));
                ctx.show_toast(ToastSpec::info("2 de 3: construindo imagem..."));
                ctx.show_toast(ToastSpec::success("3 de 3: publicado."));
            }
            _ => return,
        }
        self.disparados += 1;
        ctx.set("total_disparados", self.disparados.to_string());
    }
}

fn main() -> iced::Result {
    // O `GlacierDaemon` já liga a expiração automática dos toasts (e o
    // hot-reload); sem ele os toasts só fechariam pelo "×".
    GlacierDaemon::new()
        .title("Glacier - Toasts")
        .main(|motor| {
            if let Err(e) = motor.register(Box::new(Toasts { disparados: 0 })) {
                eprintln!("Error registering component: {}", e);
            }
            motor.set_initial_screen("toasts");
        })
        .run()
}
