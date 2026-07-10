//! UI + comportamento no MESMO arquivo: a markup e as funções (`incrementar`/
//! `decrementar`) vivem em `examples/contador_macro/contador_macro.gv`, dentro de `<script>`.
//!
//! O `<script>` agora é **Lua**, interpretado em tempo de execução (sem
//! compilar): `register_component` lê o arquivo, carrega o script e roteia cada ação
//! (`on_click`) para a função Lua homônima. As funções leem/escrevem o contexto
//! pela tabela global `ctx`, então `{contador}` na markup reflete `ctx.contador`.

use glacier_ui::GlacierDaemon;

fn main() -> iced::Result {
    GlacierDaemon::new()
        .title("Glacier - Contador (script)")
        .main(|motor| {
            if let Err(e) = motor
                .register_component("contador", "examples/contador_macro/contador_macro.gv")
            {
                eprintln!("Erro ao registrar: {}", e);
            }
            motor.set_initial_screen("contador");
        })
        .run()
}
