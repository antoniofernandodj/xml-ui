//! Lista reordenável via drag-and-drop: arraste pelo "⋮⋮" de um item para a
//! posição de outro para trocá-los de lugar. Ao soltar, `onReorder` entrega
//! ao componente a nova ordem (array JSON dos valores de `reorderKey`), que a
//! persiste no seu próprio estado — o mesmo mecanismo que o `remote-ui` do
//! rustploy usa para lembrar a ordem das env vars de um serviço.
//!
//! Rode com: `cargo run --example lista_reordenavel`

use glacier_ui::{Component, Context, GlacierDaemon, Template};
use std::collections::HashMap;

struct Tarefa {
    id: String,
    nome: String,
}

/// Componente que encapsula UI (template XML) + comportamento + estado: a
/// ordem "oficial" das tarefas vive aqui, não só no contexto.
struct ListaReordenavel {
    tarefas: Vec<Tarefa>,
}

impl ListaReordenavel {
    /// Serializa a lista para JSON e publica no contexto — é isso que o
    /// `<ForEach items="tarefas">` do template consome.
    fn sincronizar(&self, ctx: &mut Context) {
        let arr: Vec<serde_json::Value> = self
            .tarefas
            .iter()
            .map(|t| serde_json::json!({ "id": t.id, "nome": t.nome }))
            .collect();
        ctx.set("tarefas", serde_json::Value::Array(arr).to_string());
        ctx.set("total", self.tarefas.len().to_string());
    }
}

impl Component for ListaReordenavel {
    fn name(&self) -> &str {
        "lista_reordenavel"
    }

    fn template(&self) -> Template {
        Template::File("examples/lista_reordenavel/lista_reordenavel.gv".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        ctx.set("ultima_ordem", "—");
        self.sincronizar(ctx);
    }

    fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context) {
        if action == "reordenar" {
            let Some(value) = value else { return };
            let Ok(ids) = serde_json::from_str::<Vec<String>>(value) else { return };
    
            // Reordena `self.tarefas` seguindo a nova ordem de ids que o motor
            // já vinha refletindo ao vivo no contexto enquanto o usuário arrastava.
            let mut by_id: HashMap<String, Tarefa> =
                self.tarefas.drain(..).map(|t| (t.id.clone(), t)).collect();
            self.tarefas = ids.into_iter().filter_map(|id| by_id.remove(&id)).collect();
    
            ctx.set("ultima_ordem", value.to_string());
            self.sincronizar(ctx);
        }
    }
}

fn main() -> iced::Result {
    // O `GlacierDaemon` já registra o listener global de "soltar o mouse" (que
    // encerra o drag) e o hot-reload — sem wiring manual de subscription.
    GlacierDaemon::new()
        .title("Glacier - Lista Reordenável (drag-and-drop)")
        .main(|motor| {
            let tarefas = vec![
                Tarefa { id: "1".into(), nome: "Revisar PR".into() },
                Tarefa { id: "2".into(), nome: "Escrever changelog".into() },
                Tarefa { id: "3".into(), nome: "Publicar release".into() },
            ];

            if let Err(e) = motor.register(Box::new(ListaReordenavel { tarefas })) {
                eprintln!("Erro ao registrar 'lista_reordenavel': {}", e);
            }
            motor.set_initial_screen("lista_reordenavel");
        })
        .run()
}
