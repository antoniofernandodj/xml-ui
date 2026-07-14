//! Demonstra as lacunas fechadas em `PLANO_LUAU_ROBUSTEZ.md`: temporizadores
//! (`after`/`cancel`), persistência local (`storage`), leitura de viewport,
//! tabelas aceitas em `ctx` (serializadas via `json.encode`) e erros de
//! script visíveis ao usuário (`on_error`, com fallback automático em toast).
//!
//! Toda a lógica está em `robustez.luau` — este `main.rs` só registra o
//! componente. Hot-reload e expiração de toasts já vêm ligados pelo
//! `GlacierDaemon`.
//!
//! Rode com: `cargo run --example robustez_luau`

use glacier_ui::GlacierDaemon;

fn main() -> iced::Result {
    GlacierDaemon::new()
        .title("Glacier - robustez da camada Luau")
        .main(|motor| {
            if let Err(e) =
                motor.register_component("robustez", "examples/robustez_luau/robustez.gv")
            {
                eprintln!("Erro ao registrar: {}", e);
            }
            motor.set_initial_screen("robustez");
        })
        .run()
}
