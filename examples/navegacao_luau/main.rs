//! Navegação decidida pelo `<script>` Lua (`navigate`/`navigate_back`), ao
//! invés dos atributos declarativos `navigateTo`/`navigateBack` (ver o
//! exemplo `navegacao`, que só troca de tela).
//!
//! Aqui o clique em "Entrar" só navega se a validação em Lua passar — o
//! próprio botão não sabe (nem pode saber, sendo declarativo) para onde vai.
//!
//! Rode com: `cargo run --example navegacao_luau`

use glacier_ui::GlacierDaemon;

fn main() -> iced::Result {
    GlacierDaemon::new()
        .title("Glacier - navegação via script Lua")
        .main(|motor| {
            if let Err(e) = motor.register_component("login_luau", "examples/navegacao_luau/login.gv") {
                eprintln!("Erro ao registrar 'login_luau': {}", e);
            }
            if let Err(e) = motor.register_component("dashboard_luau", "examples/navegacao_luau/dashboard.gv") {
                eprintln!("Erro ao registrar 'dashboard_luau': {}", e);
            }
            motor.set_initial_screen("login_luau");
        })
        .run()
}
