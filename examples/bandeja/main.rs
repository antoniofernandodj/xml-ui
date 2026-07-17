//! **Ícone de bandeja** (system tray) + app que **sobrevive à última janela**,
//! sobre o runner [`GlacierDaemon`].
//!
//! Com `.tray(...)` configurada, fechar a janela **não encerra** o app: ele
//! recolhe para a bandeja. O menu do ícone controla o ciclo de vida:
//!
//! - **Open** — reabre (ou foca) a janela principal (`open_main`).
//! - **Disable/Enable notifications** — alterna o interruptor global das
//!   notificações do SO (`set_notifications_enabled`), e troca o próprio rótulo.
//! - **Quit** — encerra o app de vez (`quit`).
//!
//! No Windows o clique **esquerdo** no ícone também reabre a janela; no Linux não
//! há evento de clique no ícone (o clique abre o menu) — use o item "Open".
//!
//! Rode com: `cargo run --example bandeja --features tray`
//! (sem a feature `tray` a bandeja não sobe e o app encerra na última janela.)

use glacier_ui::{
    Component, Context, GlacierDaemon, Template, TrayActions, TrayConfig, TrayItem,
    notifications_enabled, set_notifications_enabled,
};

/// Ícone da bandeja, embutido no binário do exemplo.
const ICON: &[u8] = include_bytes!("icon.png");

/// Componente trivial: um botão que dispara uma notificação do SO, para dar o
/// que testar no toggle "Disable/Enable notifications" da bandeja.
struct Painel;

impl Component for Painel {
    fn name(&self) -> &str {
        "painel"
    }

    fn template(&self) -> Template {
        Template::File("examples/bandeja/painel.gv".into())
    }

    fn update(&mut self, action: &str, _value: Option<&str>, ctx: &mut Context) {
        if action == "notificar" {
            // Respeita o toggle da bandeja: desligado, isto vira no-op.
            ctx.notify(glacier_ui::component::NotificationSpec {
                title: "Bandeja".into(),
                body: "Uma notificação de teste.".into(),
                app_name: None,
                icon: None,
            });
        }
    }
}

fn main() -> iced::Result {
    GlacierDaemon::new()
        .title("Glacier — Bandeja")
        .main_size(460.0, 300.0)
        .tray(TrayConfig {
            icon: ICON.to_vec(),
            tooltip: "Glacier — exemplo de bandeja".to_string(),
            items: vec![
                TrayItem::button("open", "Open"),
                TrayItem::button("notifications", "Disable notifications"),
                TrayItem::separator(),
                TrayItem::button("quit", "Quit"),
            ],
        })
        .on_tray(|id: &str, tray: &mut TrayActions| match id {
            "open" => tray.open_main(),
            "quit" => tray.quit(),
            "notifications" => {
                // Inverte o interruptor global e reflete no rótulo do item.
                let on = !notifications_enabled();
                set_notifications_enabled(on);
                tray.set_label(
                    "notifications",
                    if on {
                        "Disable notifications"
                    } else {
                        "Enable notifications"
                    },
                );
            }
            _ => {}
        })
        .main(|motor| {
            if let Err(e) = motor.register(Box::new(Painel)) {
                eprintln!("Erro ao registrar 'painel': {e}");
            }
            motor.set_initial_screen("painel");
        })
        .run()
}
