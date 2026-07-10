//! Igual ao `contador_macro`, mas o `<script>` **aponta para um arquivo Lua
//! externo** em vez de embutir o código:
//!
//! ```xml
//! <script src="contador_externo.luau"></script>
//! ```
//!
//! O caminho é resolvido relativo ao diretório do template. `register_component` lê o
//! template, segue o `src`, carrega o Lua e roteia as ações (`on_click`/`onChange`)
//! para as funções homônimas — que leem/escrevem o contexto pela tabela `ctx`.

use glacier_ui::GlacierDaemon;

fn main() -> iced::Result {
    GlacierDaemon::new()
        .title("Glacier - Contador (script Lua externo)")
        .main(|motor| {
            if let Err(e) = motor
                .register_component("contador", "examples/contador_externo/contador_externo.gv")
            {
                eprintln!("Erro ao registrar: {}", e);
            }
            motor.set_initial_screen("contador");
        })
        .run()
}
