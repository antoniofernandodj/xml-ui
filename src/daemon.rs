//! Runner **multi-janela** do Glacier, sobre o modelo `iced::daemon`.
//!
//! No iced 0.14 múltiplas janelas exigem o `daemon` (não `application`), porque
//! só ele tem `view`/`title` indexados por [`window::Id`]. O [`GlacierDaemon`]
//! mantém **um [`GlacierUI`] por janela** (`windows`), cada um independente:
//! contexto, telas, componentes e estado isolados. Abrir uma janela nova (via
//! [`crate::Context::open_window`] no Rust ou `open_window(...)` na Lua) sobe um
//! motor fresco que carrega aquela fonte do zero.
//!
//! Uso típico no `main` de um app:
//!
//! ```ignore
//! fn main() -> iced::Result {
//!     GlacierDaemon::new()
//!         .title("Meu app")
//!         .main(|motor| {
//!             motor.register(Box::new(MinhaTela::new())).unwrap();
//!             motor.set_initial_screen("minha_tela");
//!         })
//!         .run()
//! }
//! ```

use std::collections::HashMap;
use std::time::Duration;

use iced::window;
use iced::{Element, Size, Subscription, Task};

use crate::component::{WindowSource, WindowSpec};
use crate::{EngineMessage, GlacierUI};

/// Construtor/runner do app multi-janela. Ver [módulo](self).
pub struct GlacierDaemon {
    /// Título da janela principal (e default das demais que não trazem um).
    title: String,
    /// Tamanho inicial da janela principal, em px lógicos.
    main_size: (f32, f32),
    /// Configura o motor da janela principal (registra componentes, define a
    /// tela inicial, carrega `.gss`, …). Rodado uma vez na inicialização.
    setup: Box<dyn Fn(&mut GlacierUI)>,
    /// Período do tick de hot-reload (checagem de arquivos alterados).
    reload_period: Duration,
    /// Período do tick de expiração de toasts.
    toast_period: Duration,
}

impl GlacierDaemon {
    /// Novo runner com um `setup` vazio — chame [`GlacierDaemon::main`] para
    /// configurar a janela principal antes de [`GlacierDaemon::run`].
    pub fn new() -> Self {
        Self {
            title: "Glacier".to_string(),
            main_size: (1024.0, 768.0),
            setup: Box::new(|_| {}),
            reload_period: Duration::from_millis(500),
            toast_period: Duration::from_millis(400),
        }
    }

    /// Define o título da janela principal (encadeável).
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Define o tamanho inicial da janela principal (encadeável).
    pub fn main_size(mut self, width: f32, height: f32) -> Self {
        self.main_size = (width, height);
        self
    }

    /// Registra o `setup` da janela principal: recebe o [`GlacierUI`] dela para
    /// registrar componentes, definir a tela inicial, carregar estilos, etc.
    pub fn main(mut self, setup: impl Fn(&mut GlacierUI) + 'static) -> Self {
        self.setup = Box::new(setup);
        self
    }

    /// Sobe o daemon e roda o loop do iced até a última janela fechar.
    pub fn run(self) -> iced::Result {
        let GlacierDaemon { title, main_size, setup, reload_period, toast_period } = self;
        let main_title = title.clone();

        // `boot` do iced: constrói o motor principal via `setup` e abre a janela
        // inicial. `window::open` devolve o `Id` de imediato, então já inserimos
        // o motor em `windows` com essa chave (o daemon não abre janela sozinho).
        let boot = move || {
            let mut engine = GlacierUI::new();
            setup(&mut engine);
            let settings = window::Settings {
                size: Size::new(main_size.0, main_size.1),
                ..window::Settings::default()
            };
            let (id, open) = window::open(settings);
            let mut rt = Runtime::new(reload_period, toast_period);
            rt.titles.insert(id, main_title.clone());
            rt.windows.insert(id, engine);
            (rt, open.map(DaemonMessage::Opened))
        };

        iced::daemon(boot, Runtime::update, Runtime::view)
            .title(Runtime::title)
            .theme(Runtime::theme)
            .subscription(Runtime::subscription)
            .run()
    }
}

impl Default for GlacierDaemon {
    fn default() -> Self {
        Self::new()
    }
}

/// Mensagem do daemon. Roteia eventos para o motor da janela certa. Note que
/// nenhuma variante carrega um [`WindowSpec`]/`Box<dyn Component>`: a janela é
/// materializada de imediato em `update` (com o `Id` síncrono de `window::open`),
/// então a mensagem só precisa carregar tipos `Clone`.
#[derive(Debug, Clone)]
pub enum DaemonMessage {
    /// Um [`EngineMessage`] destinado ao motor da janela `id`.
    Ui { id: window::Id, msg: EngineMessage },
    /// Uma janela terminou de abrir (retorno de `window::open`). Só informativo.
    Opened(window::Id),
    /// Uma janela foi fechada (via `window::close_events`): remove o motor e,
    /// se era a última, encerra o app.
    Closed(window::Id),
    /// Tick periódico aplicado a **todas** as janelas (hot-reload, expiração de
    /// toasts) — cada motor checa os próprios arquivos/toasts.
    TickAll(EngineMessage),
}

/// Estado do daemon: um motor por janela + seus títulos.
struct Runtime {
    windows: HashMap<window::Id, GlacierUI>,
    titles: HashMap<window::Id, String>,
    reload_period: Duration,
    toast_period: Duration,
}

impl Runtime {
    fn new(reload_period: Duration, toast_period: Duration) -> Self {
        Self {
            windows: HashMap::new(),
            titles: HashMap::new(),
            reload_period,
            toast_period,
        }
    }

    fn update(&mut self, message: DaemonMessage) -> Task<DaemonMessage> {
        match message {
            DaemonMessage::Ui { id, msg } => self.route(id, msg),
            DaemonMessage::Opened(_) => Task::none(),
            DaemonMessage::Closed(id) => {
                self.windows.remove(&id);
                self.titles.remove(&id);
                if self.windows.is_empty() {
                    iced::exit()
                } else {
                    Task::none()
                }
            }
            DaemonMessage::TickAll(msg) => {
                // Aplica o tick a cada janela (clonando a mensagem por janela).
                let ids: Vec<window::Id> = self.windows.keys().copied().collect();
                let tasks: Vec<_> = ids.into_iter().map(|id| self.route(id, msg.clone())).collect();
                Task::batch(tasks)
            }
        }
    }

    /// Despacha `msg` ao motor da janela `id` e, em seguida, abre quaisquer
    /// janelas que aquele motor tenha pedido durante o `dispatch`.
    fn route(&mut self, id: window::Id, msg: EngineMessage) -> Task<DaemonMessage> {
        // 1. despacha ao motor da janela (borrow escopado)
        let ui_task = match self.windows.get_mut(&id) {
            Some(engine) => engine.dispatch(&msg).map(move |m| DaemonMessage::Ui { id, msg: m }),
            None => return Task::none(),
        };
        let mut tasks = vec![ui_task];

        // 2. drena os pedidos de janela nova desse mesmo motor e abre cada um
        let pending = self
            .windows
            .get_mut(&id)
            .map(|e| e.take_pending_windows())
            .unwrap_or_default();
        for spec in pending {
            tasks.push(self.open_child(spec));
        }

        // 3. drena os broadcasts desse motor e entrega às OUTRAS janelas
        let broadcasts = self
            .windows
            .get_mut(&id)
            .map(|e| e.take_pending_broadcasts())
            .unwrap_or_default();
        if !broadcasts.is_empty() {
            let others: Vec<window::Id> =
                self.windows.keys().copied().filter(|w| *w != id).collect();
            for b in &broadcasts {
                for &oid in &others {
                    if let Some(engine) = self.windows.get_mut(&oid) {
                        tasks.push(
                            engine
                                .deliver_broadcast(&b.event, &b.payload)
                                .map(move |m| DaemonMessage::Ui { id: oid, msg: m }),
                        );
                    }
                }
            }
        }

        // 4. se o motor pediu para fechar a própria janela, fecha (o
        // `close_events` subsequente remove o motor; a última encerra o app)
        if self.windows.get_mut(&id).map(|e| e.take_close_requested()).unwrap_or(false) {
            tasks.push(window::close(id));
        }

        Task::batch(tasks)
    }

    /// Materializa um [`WindowSpec`] numa janela nova: constrói um motor fresco,
    /// abre a janela (o `Id` vem síncrono) e registra motor + título.
    fn open_child(&mut self, spec: WindowSpec) -> Task<DaemonMessage> {
        let WindowSpec { source, title, size, resizable, data } = spec;
        let (engine, fallback_title) = build_engine(source, &data);
        let (w, h) = size.unwrap_or((640.0, 480.0));
        let settings = window::Settings {
            size: Size::new(w, h),
            resizable,
            ..window::Settings::default()
        };
        let (id, open) = window::open(settings);
        self.titles.insert(id, title.unwrap_or(fallback_title));
        self.windows.insert(id, engine);
        open.map(DaemonMessage::Opened)
    }

    fn view(&self, id: window::Id) -> Element<'_, DaemonMessage> {
        match self.windows.get(&id) {
            Some(engine) => match engine.render_current() {
                Ok(elem) => elem.map(move |msg| DaemonMessage::Ui { id, msg }),
                Err(e) => iced::widget::text(format!("Erro ao renderizar: {e}"))
                    .color(iced::Color::from_rgb(1.0, 0.0, 0.0))
                    .into(),
            },
            None => iced::widget::text("").into(),
        }
    }

    fn title(&self, id: window::Id) -> String {
        self.titles.get(&id).cloned().unwrap_or_else(|| "Glacier".to_string())
    }

    fn theme(&self, id: window::Id) -> iced::Theme {
        self.windows.get(&id).map(|e| e.theme()).unwrap_or(iced::Theme::Dark)
    }

    fn subscription(&self) -> Subscription<DaemonMessage> {
        // Listeners globais de evento, registrados UMA vez no daemon: usam o
        // `window::Id` que o callback recebe para rotear ao motor certo. Se cada
        // motor os registrasse, o iced fundiria os recipes idênticos num só.
        let mut subs = vec![
            iced::event::listen_with(|e, s, id| {
                crate::drag_end_from_event(e, s, id).map(|msg| DaemonMessage::Ui { id, msg })
            }),
            iced::event::listen_with(|e, s, id| {
                crate::tab_focus_from_event(e, s, id).map(|msg| DaemonMessage::Ui { id, msg })
            }),
            iced::event::listen_with(|e, s, id| {
                crate::viewport_from_event(e, s, id).map(|msg| DaemonMessage::Ui { id, msg })
            }),
            window::close_events().map(DaemonMessage::Closed),
            iced::time::every(self.reload_period)
                .map(|_| DaemonMessage::TickAll(EngineMessage::FileChanged(String::new()))),
            iced::time::every(self.toast_period)
                .map(|_| DaemonMessage::TickAll(EngineMessage::ToastTick)),
        ];

        // Subscriptions por-motor (streams `sse`/`websocket`, `Component::subscription`):
        // marcadas com o `id` da janela. Streams já vêm isolados por `engine_id`.
        // `Subscription::map` exige um closure não-capturante; para embutir o
        // `id` da janela usamos `.with(id)` (que emite `(id, msg)`) e um map sem
        // captura.
        for (id, engine) in &self.windows {
            subs.push(
                engine
                    .subscription()
                    .with(*id)
                    .map(|(id, msg)| DaemonMessage::Ui { id, msg }),
            );
        }
        Subscription::batch(subs)
    }
}

/// Constrói um [`GlacierUI`] novo para uma janela a partir da sua fonte, e
/// devolve também o título de fallback (nome do componente). `Named` já deve ter
/// sido resolvido para `File` no motor de origem (ver `run_on_owner`). `data`
/// (pares `open_window({ data = ... })`) é semeado no contexto **antes** de
/// registrar o componente, para que seu `init` já enxergue os valores.
fn build_engine(source: WindowSource, data: &[(String, String)]) -> (GlacierUI, String) {
    let mut engine = GlacierUI::new();
    for (k, v) in data {
        engine.define_data(k, v);
    }
    let title = match source {
        WindowSource::Component(comp) => {
            let name = comp.name().to_string();
            if let Err(e) = engine.register(comp) {
                eprintln!("open_window: falha ao registrar componente: {e}");
            }
            engine.set_initial_screen(&name);
            name
        }
        WindowSource::File(path) => {
            let name = file_stem(&path);
            if let Err(e) = engine.register_component(&name, &path) {
                eprintln!("open_window: falha ao carregar '{path}': {e}");
            }
            engine.set_initial_screen(&name);
            name
        }
        WindowSource::Named(name) => {
            // Não deveria acontecer: `run_on_owner` resolve `Named` para `File`.
            eprintln!("open_window: fonte 'Named({name})' não resolvida; janela vazia");
            name
        }
    };
    (engine, title)
}

/// Nome de componente derivado do caminho de um arquivo (o stem, sem extensão).
fn file_stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("janela")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{Component, Context, Template};
    use crate::EngineMessage;

    /// Componente de teste: cada ação pede uma janela nova de tipo diferente.
    struct Abridor;
    impl Component for Abridor {
        fn name(&self) -> &str {
            "abridor"
        }
        fn template(&self) -> Template {
            Template::Inline("<Text content=\"x\" />".to_string())
        }
        fn update(&mut self, action: &str, _v: Option<&str>, ctx: &mut Context) {
            match action {
                "rust" => ctx.open_window_component(Box::new(Abridor)),
                "arquivo" => ctx.open_window(
                    WindowSpec::file("examples/janelas_glacier/detalhe.gv").title("D"),
                ),
                "nomeado" => ctx.open_window(WindowSpec::named("detalhe")),
                _ => {}
            }
        }
    }

    #[test]
    fn open_window_component_vira_pending_window() {
        let mut motor = GlacierUI::new();
        motor.register(Box::new(Abridor)).unwrap();
        motor.set_initial_screen("abridor");

        // Antes de qualquer ação, nada pendente.
        assert!(motor.take_pending_windows().is_empty());

        // A ação Rust deve enfileirar uma janela com fonte Component.
        let _ = motor.dispatch(&EngineMessage::UiClick("rust".into()));
        let pending = motor.take_pending_windows();
        assert_eq!(pending.len(), 1);
        assert!(matches!(pending[0].source, WindowSource::Component(_)));

        // A ação de arquivo enfileira uma janela File com título.
        let _ = motor.dispatch(&EngineMessage::UiClick("arquivo".into()));
        let pending = motor.take_pending_windows();
        assert_eq!(pending.len(), 1);
        assert!(matches!(&pending[0].source, WindowSource::File(p) if p.ends_with("detalhe.gv")));
        assert_eq!(pending[0].title.as_deref(), Some("D"));
    }

    #[test]
    fn open_window_named_resolve_para_arquivo() {
        let mut motor = GlacierUI::new();
        // Registra "detalhe" como componente de arquivo; a resolução Named→File
        // acontece na drenagem do Context (ver `run_on_owner`).
        motor
            .register_component("detalhe", "examples/janelas_glacier/detalhe.gv")
            .unwrap();
        motor.register(Box::new(Abridor)).unwrap();
        motor.set_initial_screen("abridor");

        let _ = motor.dispatch(&EngineMessage::UiClick("nomeado".into()));
        let pending = motor.take_pending_windows();
        assert_eq!(pending.len(), 1);
        match &pending[0].source {
            WindowSource::File(p) => assert_eq!(p, "examples/janelas_glacier/detalhe.gv"),
            _ => panic!("Named deveria ter sido resolvido para File"),
        }
    }

    #[test]
    fn build_engine_de_arquivo_usa_stem_como_titulo() {
        let (engine, title) =
            build_engine(WindowSource::File("examples/janelas_glacier/detalhe.gv".into()), &[]);
        assert_eq!(title, "detalhe");
        // O motor da nova janela renderiza a tela carregada sem erro.
        assert!(engine.render_current().is_ok());
    }

    #[test]
    fn build_engine_semeia_data_no_contexto() {
        let (engine, _) = build_engine(
            WindowSource::File("examples/janelas_glacier/detalhe.gv".into()),
            &[("url".into(), "http://x".into()), ("token".into(), "abc".into())],
        );
        assert_eq!(engine.get_data("url").map(String::as_str), Some("http://x"));
        assert_eq!(engine.get_data("token").map(String::as_str), Some("abc"));
    }

    /// Emissor: uma ação envia um broadcast. Receptor: registra o que recebe.
    struct Emissor;
    impl Component for Emissor {
        fn name(&self) -> &str {
            "emissor"
        }
        fn template(&self) -> Template {
            Template::Inline("<Text content=\"x\" />".to_string())
        }
        fn update(&mut self, action: &str, _v: Option<&str>, ctx: &mut Context) {
            match action {
                "enviar" => ctx.broadcast("ping", "{\"v\":\"1\"}"),
                "fechar" => ctx.close_window(),
                _ => {}
            }
        }
    }
    struct Receptor;
    impl Component for Receptor {
        fn name(&self) -> &str {
            "receptor"
        }
        fn template(&self) -> Template {
            Template::Inline("<Text content=\"x\" />".to_string())
        }
        fn update(&mut self, _a: &str, _v: Option<&str>, _c: &mut Context) {}
        fn on_broadcast(&mut self, event: &str, payload: &str, ctx: &mut Context) {
            ctx.set("rx", format!("{event}:{payload}"));
        }
    }

    #[test]
    fn broadcast_de_um_motor_chega_no_on_broadcast_de_outro() {
        // Motor emissor: a ação enfileira um broadcast pendente.
        let mut a = GlacierUI::new();
        a.register(Box::new(Emissor)).unwrap();
        a.set_initial_screen("emissor");
        assert!(a.take_pending_broadcasts().is_empty());
        let _ = a.dispatch(&EngineMessage::UiClick("enviar".into()));
        let msgs = a.take_pending_broadcasts();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].event, "ping");

        // Motor receptor: `deliver_broadcast` chama seu `on_broadcast`.
        let mut b = GlacierUI::new();
        b.register(Box::new(Receptor)).unwrap();
        b.set_initial_screen("receptor");
        let _ = b.deliver_broadcast(&msgs[0].event, &msgs[0].payload);
        assert_eq!(b.get_data("rx").map(String::as_str), Some("ping:{\"v\":\"1\"}"));
    }

    #[test]
    fn close_window_vira_take_close_requested() {
        let mut a = GlacierUI::new();
        a.register(Box::new(Emissor)).unwrap();
        a.set_initial_screen("emissor");
        assert!(!a.take_close_requested());
        let _ = a.dispatch(&EngineMessage::UiClick("fechar".into()));
        assert!(a.take_close_requested());
        // Consumido: não persiste.
        assert!(!a.take_close_requested());
    }
}
