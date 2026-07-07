use glacier_ui::{GlacierUI, EngineMessage, Component, Context, Template};
use iced::{Element, Task};
use std::time::Duration;

/// Demonstra os pseudo-estados de `.gss` (`:hover` / `:focus` / `:active` /
/// `:disabled`, ver PLANO_GSS_LIMITACOES.md item 7):
///
/// - **Button** — `.btn:hover`/`:active`/`:disabled` completos. Passe o mouse,
///   segure o clique e compare com a versão `disabled="true"`.
/// - **TextInput** — `.input:hover`/`:focus`/`:disabled` completos. Clique
///   para focar e veja a borda mudar; compare com a versão desabilitada.
/// - **Select** — só `.select:hover` (o `pick_list` do iced não tem um
///   `Status::Disabled`).
/// - **Checkbox/Toggle** — só o atributo `disabled` (o overlay de cor por
///   estado ainda não está implementado para esses dois; usam o visual
///   padrão do tema quando desabilitados).
///
/// `disabled="true"` é sempre um atributo **inline estático** (sem
/// equivalente `.classe { }`, e sem interpolação de contexto como
/// `{variavel}` — mesma limitação de `hidden`), por isso os widgets
/// desabilitados aqui são cópias fixas lado a lado, não um único widget
/// alternando de estado.
///
/// A stylesheet (`pseudo_estados.gss`) e o template têm hot-reload: edite-os
/// com o exemplo rodando.
struct PseudoEstados;

impl Component for PseudoEstados {
    fn name(&self) -> &str { "pseudo_estados" }

    fn template(&self) -> Template {
        Template::File("examples/pseudo_estados/pseudo_estados.xml".into())
    }

    fn init(&mut self, ctx: &mut Context) {
        ctx.set("texto", "");
        ctx.set("cores", r#"["Azul", "Verde", "Vermelho", "Roxo"]"#);
        ctx.set("cor_selecionada", "Azul");
        ctx.set("marcado", "false");
        ctx.set("marcado_travado", "true");
        ctx.set("ligado", "false");
        ctx.set("ligado_travado", "true");
    }

    fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context) {
        match action {
            // TextInput/Select/Checkbox/Toggle usam o próprio nome da chave
            // de contexto como ação — o handler só faz o round-trip.
            "texto" | "cor_selecionada" | "marcado" | "marcado_travado" | "ligado" | "ligado_travado" => {
                ctx.set(action, value.unwrap_or_default());
            }
            "enviar" => {
                ctx.set("texto", "enviado!");
            }
            _ => {}
        }
    }
}

struct AppPseudoEstados {
    motor: GlacierUI,
}

impl AppPseudoEstados {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = GlacierUI::new();
        if let Err(e) = motor.register(Box::new(PseudoEstados)) {
            eprintln!("Erro ao registrar 'pseudo_estados': {}", e);
        }
        if let Err(e) = motor.load_stylesheet("examples/pseudo_estados/pseudo_estados.gss") {
            eprintln!("Erro ao carregar stylesheet: {}", e);
        }
        motor.set_initial_screen("pseudo_estados");

        (Self { motor }, Task::none())
    }

    fn update(&mut self, message: EngineMessage) -> Task<EngineMessage> {
        self.motor.dispatch(&message)
    }

    fn view(&self) -> Element<'_, EngineMessage> {
        self.motor.render_current().unwrap_or_else(|e| {
            iced::widget::text(format!("Erro ao renderizar: {}", e))
                .color(iced::Color::from_rgb(1.0, 0.0, 0.0))
                .into()
        })
    }

    fn subscription(&self) -> iced::Subscription<EngineMessage> {
        GlacierUI::reload_subscription(Duration::from_millis(500))
    }

    fn theme(&self) -> iced::Theme {
        self.motor.theme()
    }
}

fn main() -> iced::Result {
    iced::application(|| AppPseudoEstados::new(), AppPseudoEstados::update, AppPseudoEstados::view)
        .subscription(AppPseudoEstados::subscription)
        .theme(AppPseudoEstados::theme)
        .title("Glacier - Pseudo-estados (:hover/:focus/:active/:disabled)")
        .run()
}
