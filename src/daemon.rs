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
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use iced::window;
use iced::{Element, Font, Point, Size, Subscription, Task};

use crate::component::{WindowSource, WindowSpec};
use crate::tray::{TrayActions, TrayConfig, TrayHandle, TrayMsg, TrayRequest};
use crate::{EngineMessage, GlacierUI};

/// A geometria de uma janela no momento em que ela vai fechar — o que um app
/// precisa para reabrir onde parou. Entregue ao gancho de
/// [`GlacierDaemon::on_close`].
///
/// `position` é `None` no Wayland: o protocolo simplesmente não expõe a posição
/// da janela ao cliente. Não é um bug a corrigir; é para o app decidir o que
/// fazer (na prática, só persistir o tamanho).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowGeometry {
    pub size: Size,
    pub position: Option<Point>,
}

/// Ajuste das `window::Settings` de uma janela-filha, recebendo o [`WindowSpec`]
/// que a pediu. Ver [`GlacierDaemon::child_window`].
type ChildSettingsHook = Rc<dyn Fn(&WindowSpec, &mut window::Settings)>;
/// Observador de cada mensagem despachada na janela principal, com o motor no
/// estado resultante. Ver [`GlacierDaemon::on_message`].
type MessageHook = Rc<dyn Fn(&EngineMessage, &GlacierUI)>;
/// Gancho de fechamento da janela principal, com a geometria dela. Ver
/// [`GlacierDaemon::on_close`].
type CloseHook = Rc<dyn Fn(&GlacierUI, WindowGeometry)>;
/// Configuração do motor da janela principal (registra componentes, tela
/// inicial, …). É um `Rc` para o `Runtime` guardá-la e **reabrir** a principal
/// depois que ela fecha (o clique "Open Rustploy" da bandeja). Ver
/// [`GlacierDaemon::main`].
type SetupHook = Rc<dyn Fn(&mut GlacierUI)>;
/// Gancho de clique num item do menu da bandeja: recebe o `id` do item e um
/// [`TrayActions`] para pedir ações (abrir a principal, sair, mudar rótulo). Ver
/// [`GlacierDaemon::on_tray`].
type TrayHook = Rc<dyn Fn(&str, &mut TrayActions)>;

/// Construtor/runner do app multi-janela. Ver [módulo](self).
pub struct GlacierDaemon {
    /// Título da janela principal (e default das demais que não trazem um).
    title: String,
    /// `window::Settings` da janela principal. Começa no default do iced com o
    /// tamanho de [`GlacierDaemon::main_size`]; um app que precise de mais
    /// (borderless, ícone, `min_size`, geometria restaurada) troca o bloco
    /// inteiro com [`GlacierDaemon::main_window`].
    main_settings: window::Settings,
    /// Ajuste opcional das `window::Settings` de cada janela-filha, aplicado
    /// sobre o default do daemon. Ver [`GlacierDaemon::child_window`].
    child_settings: Option<ChildSettingsHook>,
    /// Configura o motor da janela principal (registra componentes, define a
    /// tela inicial, carrega `.gss`, …). Rodado na inicialização e de novo a
    /// cada reabertura da principal pela bandeja.
    setup: SetupHook,
    /// Fontes embutidas a registrar no runtime do iced (bytes de `.ttf`/`.otf`).
    fonts: Vec<&'static [u8]>,
    /// Fonte padrão de todas as janelas, quando o app embute a sua.
    default_font: Option<Font>,
    /// Observador rodado depois de cada `dispatch` na janela principal — é o
    /// gancho de persistência (ver [`GlacierDaemon::on_message`]).
    on_message: Option<MessageHook>,
    /// Gancho de fechamento da janela principal (ver [`GlacierDaemon::on_close`]).
    on_close: Option<CloseHook>,
    /// Período do tick de hot-reload (checagem de arquivos alterados).
    reload_period: Duration,
    /// Período do tick de expiração de toasts.
    toast_period: Duration,
    /// Raiz opcional onde o global `storage` (persistência local em JSON) grava
    /// seus arquivos, aplicada a todos os motores. Sem isto, `storage` grava
    /// relativo ao diretório do script — inviável quando os assets moram num
    /// diretório read-only. Ver [`GlacierDaemon::storage_dir`].
    storage_dir: Option<PathBuf>,
    /// Configuração da bandeja (ícone + menu), quando o app quer sobreviver à
    /// última janela. Ver [`GlacierDaemon::tray`].
    tray_config: Option<TrayConfig>,
    /// Gancho de clique nos itens da bandeja. Ver [`GlacierDaemon::on_tray`].
    on_tray: Option<TrayHook>,
}

impl GlacierDaemon {
    /// Novo runner com um `setup` vazio — chame [`GlacierDaemon::main`] para
    /// configurar a janela principal antes de [`GlacierDaemon::run`].
    pub fn new() -> Self {
        Self {
            title: "Glacier".to_string(),
            main_settings: window::Settings {
                size: Size::new(1024.0, 768.0),
                ..window::Settings::default()
            },
            child_settings: None,
            setup: Rc::new(|_| {}),
            fonts: Vec::new(),
            default_font: None,
            on_message: None,
            on_close: None,
            reload_period: Duration::from_millis(500),
            toast_period: Duration::from_millis(400),
            storage_dir: None,
            tray_config: None,
            on_tray: None,
        }
    }

    /// Define o título da janela principal (encadeável).
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Define o tamanho inicial da janela principal (encadeável).
    pub fn main_size(mut self, width: f32, height: f32) -> Self {
        self.main_settings.size = Size::new(width, height);
        self
    }

    /// Substitui as `window::Settings` da janela principal — o escape hatch para
    /// tudo que o builder não nomeia: `decorations: false` (titlebar própria),
    /// `icon`, `min_size`, `position` restaurada, `platform_specific`.
    ///
    /// Um app com titlebar custom também vai querer `exit_on_close_request:
    /// false`, para que o pedido de fechar da WM passe por
    /// [`GlacierDaemon::on_close`] antes de a janela sumir.
    pub fn main_window(mut self, settings: window::Settings) -> Self {
        self.main_settings = settings;
        self
    }

    /// Ajusta as `window::Settings` de cada janela-filha (as abertas por
    /// `open_window(...)`), recebendo o [`WindowSpec`] que a pediu. Sem isto,
    /// filhas nascem com o default do iced — o que destoa num app borderless,
    /// onde elas precisam do mesmo `decorations: false` da principal.
    pub fn child_window(
        mut self,
        f: impl Fn(&WindowSpec, &mut window::Settings) + 'static,
    ) -> Self {
        self.child_settings = Some(Rc::new(f));
        self
    }

    /// Embute uma fonte (bytes de um `.ttf`/`.otf`) no binário e a registra no
    /// iced. Encadeável — chame uma vez por peso (regular, bold, …).
    pub fn font(mut self, bytes: &'static [u8]) -> Self {
        self.fonts.push(bytes);
        self
    }

    /// Define a fonte padrão de todas as janelas (tipicamente uma embutida com
    /// [`GlacierDaemon::font`]).
    pub fn default_font(mut self, font: Font) -> Self {
        self.default_font = Some(font);
        self
    }

    /// Registra o `setup` da janela principal: recebe o [`GlacierUI`] dela para
    /// registrar componentes, definir a tela inicial, carregar estilos, etc.
    pub fn main(mut self, setup: impl Fn(&mut GlacierUI) + 'static) -> Self {
        self.setup = Rc::new(setup);
        self
    }

    /// Habilita um **ícone de bandeja** (system tray) — e, com ele, um app que
    /// **sobrevive à última janela**: com bandeja configurada, fechar a última
    /// janela não encerra mais o app; ele recolhe para a bandeja, e só o gancho
    /// [`GlacierDaemon::on_tray`] (via `quit()`) o encerra. Sem bandeja, o
    /// comportamento é o de sempre (encerra na última janela).
    ///
    /// A bandeja sobe numa thread própria (ver [`crate::tray`]); em plataformas
    /// sem suporte (macOS, ou build sem a feature `tray`) isto é ignorado e o app
    /// volta a encerrar na última janela.
    pub fn tray(mut self, config: TrayConfig) -> Self {
        self.tray_config = Some(config);
        self
    }

    /// Gancho de clique nos itens do menu da bandeja: recebe o `id` do item e um
    /// [`TrayActions`] para pedir `open_main()`, `quit()` ou atualizar o menu
    /// (`set_label`/`set_checked`). Sem `on_tray`, os cliques não fazem nada.
    pub fn on_tray(mut self, f: impl Fn(&str, &mut TrayActions) + 'static) -> Self {
        self.on_tray = Some(Rc::new(f));
        self
    }

    /// Período do tick de hot-reload (checagem de arquivos alterados em disco).
    /// Padrão: 500ms.
    pub fn reload_period(mut self, period: Duration) -> Self {
        self.reload_period = period;
        self
    }

    /// Período do tick que expira toasts. Padrão: 400ms — mais curto deixa a
    /// expiração mais pontual, ao custo de acordar o loop mais vezes.
    pub fn toast_period(mut self, period: Duration) -> Self {
        self.toast_period = period;
        self
    }

    /// Observa cada mensagem já **despachada** na janela principal, com o motor
    /// no estado resultante. É o gancho de persistência: a camada Luau não tem
    /// I/O de arquivo, então salvar preferências (um "lembrar meu login") passa
    /// por aqui — o script grava no contexto, e o app lê o contexto e persiste.
    ///
    /// Roda **depois** do dispatch, de propósito: o interesse é o estado novo,
    /// não o velho.
    pub fn on_message(mut self, f: impl Fn(&EngineMessage, &GlacierUI) + 'static) -> Self {
        self.on_message = Some(Rc::new(f));
        self
    }

    /// Roda antes de a janela principal fechar, com a geometria dela — para
    /// persistir tamanho/posição e reabrir onde parou.
    ///
    /// A geometria é **consultada na hora** (uma ida ao runtime do iced), não
    /// acumulada de eventos `Resized`/`Moved`. A diferença é prática: durante o
    /// handshake de configuração do xdg-shell no Wayland chega um `Resized`
    /// espúrio com o `min_size` da janela, e um valor rastreado de eventos
    /// nasce envenenado com o mínimo antes de o usuário tocar em nada.
    /// Perguntar "qual é o tamanho agora?" no instante de fechar não tem essa
    /// janela de obsolescência.
    ///
    /// Só dispara se a janela principal tiver `exit_on_close_request: false`
    /// (ver [`GlacierDaemon::main_window`]) — senão o iced a fecha sozinho, sem
    /// passar por aqui.
    pub fn on_close(mut self, f: impl Fn(&GlacierUI, WindowGeometry) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// Define o diretório onde o global `storage` (persistência local em JSON,
    /// análoga a `localStorage`) grava seus arquivos — aplicado a todas as
    /// janelas do app. Passe um diretório gravável pelo usuário (ex.: o data dir
    /// do XDG). Sem isto, `storage` grava em `.glacier-storage/` relativo ao
    /// diretório do script, o que falha silenciosamente quando os assets moram
    /// num caminho read-only (um app empacotado rodando de `/usr/share`).
    pub fn storage_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.storage_dir = Some(dir.into());
        self
    }

    /// Sobe o daemon e roda o loop do iced até a última janela fechar.
    pub fn run(self) -> iced::Result {
        let GlacierDaemon {
            title,
            main_settings,
            child_settings,
            setup,
            fonts,
            default_font,
            on_message,
            on_close,
            reload_period,
            toast_period,
            storage_dir,
            tray_config,
            on_tray,
        } = self;
        let main_title = title.clone();

        // Semeia a raiz do `storage` ANTES de qualquer motor ser construído (o
        // `boot` abaixo e cada janela-filha em `build_engine` instalam o global
        // `storage` no registro do componente, lendo esta raiz já definida).
        if let Some(dir) = storage_dir {
            crate::luau::set_storage_root(dir);
        }

        // `boot` do iced: constrói o motor principal via `setup` e abre a janela
        // inicial. `window::open` devolve o `Id` de imediato, então já inserimos
        // o motor em `windows` com essa chave (o daemon não abre janela sozinho),
        // e guardamos esse `Id` como o da principal — ver `Runtime::main_id`.
        let boot = move || {
            let mut engine = GlacierUI::new();
            setup(&mut engine);
            let (id, open) = window::open(main_settings.clone());
            let mut rt = Runtime::new(
                reload_period,
                toast_period,
                id,
                setup.clone(),
                main_settings.clone(),
                main_title.clone(),
            );
            rt.child_settings = child_settings.clone();
            rt.on_message = on_message.clone();
            rt.on_close = on_close.clone();
            // Sobe a bandeja (thread própria) uma vez, no boot. Só a
            // configuração é `move`d para cá; a thread devolve a alça de
            // comandos, guardada para as atualizações de menu e o shutdown.
            if let Some(cfg) = tray_config.clone() {
                rt.tray = crate::tray::spawn(cfg);
                rt.on_tray = on_tray.clone();
            }
            rt.titles.insert(id, main_title.clone());
            rt.windows.insert(id, engine);
            (rt, open.map(DaemonMessage::Opened))
        };

        let mut app = iced::daemon(boot, Runtime::update, Runtime::view)
            .title(Runtime::title)
            .theme(Runtime::theme)
            .subscription(Runtime::subscription);
        for bytes in fonts {
            app = app.font(bytes);
        }
        if let Some(font) = default_font {
            app = app.default_font(font);
        }
        app.run()
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
    /// A OS/WM pediu para fechar uma janela (`window::close_requests`, ANTES do
    /// fechamento). Na principal, dá a chance de [`GlacierDaemon::on_close`]
    /// rodar com a geometria; nas demais, fecha direto.
    CloseRequested(window::Id),
    /// A geometria consultada em resposta a um `CloseRequested` da principal
    /// chegou: entrega-a ao gancho `on_close` e então fecha a janela.
    CloseWithGeometry(window::Id, Size, Option<Point>),
    /// Tick periódico aplicado a **todas** as janelas (hot-reload, expiração de
    /// toasts) — cada motor checa os próprios arquivos/toasts.
    TickAll(EngineMessage),
    /// Um evento da bandeja (clique de menu ou no ícone). Ver [`crate::tray`].
    Tray(TrayMsg),
}

/// Estado do daemon: um motor por janela + seus títulos.
struct Runtime {
    windows: HashMap<window::Id, GlacierUI>,
    titles: HashMap<window::Id, String>,
    /// `Id` da janela principal, conhecido já no `boot` (`window::open` o
    /// devolve síncrono). Tê-lo em mãos evita um round-trip `window::latest()`
    /// por ação de janela — e no Wayland esse adiamento **quebra** o arrasto:
    /// o compositor exige que `window::drag` seja pedido com o serial do
    /// pointer-grab ainda vivo, e um round-trip o perde, fazendo o
    /// `onPress="window:drag"` da titlebar custom virar um no-op silencioso.
    main_id: window::Id,
    child_settings: Option<ChildSettingsHook>,
    on_message: Option<MessageHook>,
    on_close: Option<CloseHook>,
    reload_period: Duration,
    toast_period: Duration,
    /// Alça da bandeja, `Some` quando ela subiu. Enquanto for `Some`, o app
    /// **não** encerra ao fechar a última janela (recolhe para a bandeja).
    tray: Option<TrayHandle>,
    /// Gancho de clique dos itens da bandeja.
    on_tray: Option<TrayHook>,
    /// O `setup`/`settings`/`título` da principal, guardados para **reabri-la**
    /// (o "Open Rustploy" da bandeja) idêntica à do boot.
    main_setup: SetupHook,
    main_settings: window::Settings,
    main_title: String,
}

impl Runtime {
    fn new(
        reload_period: Duration,
        toast_period: Duration,
        main_id: window::Id,
        main_setup: SetupHook,
        main_settings: window::Settings,
        main_title: String,
    ) -> Self {
        Self {
            windows: HashMap::new(),
            titles: HashMap::new(),
            main_id,
            child_settings: None,
            on_message: None,
            on_close: None,
            reload_period,
            toast_period,
            tray: None,
            on_tray: None,
            main_setup,
            main_settings,
            main_title,
        }
    }

    fn update(&mut self, message: DaemonMessage) -> Task<DaemonMessage> {
        match message {
            DaemonMessage::Ui { id, msg } => self.route(id, msg),
            DaemonMessage::Opened(_) => Task::none(),
            DaemonMessage::Closed(id) => {
                self.windows.remove(&id);
                self.titles.remove(&id);
                // Com bandeja, a última janela fechada NÃO encerra o app: ele
                // recolhe para a bandeja e só sai pelo "Quit" dela. Sem bandeja,
                // o comportamento é o de sempre.
                if self.windows.is_empty() && self.tray.is_none() {
                    iced::exit()
                } else {
                    Task::none()
                }
            }
            // A WM pediu para fechar (Alt+F4, botão da barra, fim de sessão).
            DaemonMessage::CloseRequested(id) => self.close(id),
            DaemonMessage::CloseWithGeometry(id, size, position) => {
                if let (Some(hook), Some(engine)) = (&self.on_close, self.windows.get(&id)) {
                    hook(engine, WindowGeometry { size, position });
                }
                window::close(id)
            }
            DaemonMessage::TickAll(msg) => {
                // Aplica o tick a cada janela (clonando a mensagem por janela).
                let ids: Vec<window::Id> = self.windows.keys().copied().collect();
                let tasks: Vec<_> = ids
                    .into_iter()
                    .map(|id| self.route(id, msg.clone()))
                    .collect();
                Task::batch(tasks)
            }
            DaemonMessage::Tray(msg) => self.on_tray(msg),
        }
    }

    /// Trata um evento da bandeja. O gancho `on_tray` roda com um
    /// [`TrayActions`] e só **registra** a intenção (abrir a principal / sair);
    /// aqui a traduzimos numa `Task`. O borrow imutável do gancho/handle termina
    /// antes de mexermos em `self` (abrir janela), por isso o `request` é
    /// extraído primeiro.
    fn on_tray(&mut self, msg: TrayMsg) -> Task<DaemonMessage> {
        let request = match msg {
            // Clique esquerdo no ícone (Windows): reabre a principal direto.
            TrayMsg::IconLeftClick => Some(TrayRequest::OpenMain),
            TrayMsg::Menu(id) => match (&self.on_tray, &self.tray) {
                (Some(hook), Some(handle)) => {
                    let mut actions = TrayActions::new(handle);
                    hook(&id, &mut actions);
                    actions.request
                }
                _ => None,
            },
        };
        match request {
            Some(TrayRequest::OpenMain) => self.open_main(),
            Some(TrayRequest::Quit) => {
                if let Some(tray) = &self.tray {
                    tray.shutdown();
                }
                iced::exit()
            }
            None => Task::none(),
        }
    }

    /// Reabre (ou foca, se já viva) a janela principal — o "Open Rustploy" da
    /// bandeja. Depois de a principal ter fechado, `main_id` aponta para uma
    /// janela que não está mais em `windows`, então caímos no ramo de recriar:
    /// um motor fresco via o `setup` guardado, abrindo a janela com as mesmas
    /// `settings` (geometria restaurada) do boot.
    fn open_main(&mut self) -> Task<DaemonMessage> {
        if self.windows.contains_key(&self.main_id) {
            return window::gain_focus(self.main_id);
        }
        let mut engine = GlacierUI::new();
        (self.main_setup)(&mut engine);
        let (id, open) = window::open(self.main_settings.clone());
        self.main_id = id;
        self.titles.insert(id, self.main_title.clone());
        self.windows.insert(id, engine);
        open.map(DaemonMessage::Opened)
    }

    /// Fecha a janela `id`. Na principal, e havendo um gancho `on_close`,
    /// primeiro **consulta** a geometria de verdade (ver
    /// [`GlacierDaemon::on_close`]) e só fecha depois de entregá-la.
    fn close(&mut self, id: window::Id) -> Task<DaemonMessage> {
        if id != self.main_id || self.on_close.is_none() {
            return window::close(id);
        }
        window::size(id).then(move |size| {
            window::position(id)
                .map(move |position| DaemonMessage::CloseWithGeometry(id, size, position))
        })
    }

    /// Despacha `msg` ao motor da janela `id` e, em seguida, abre quaisquer
    /// janelas que aquele motor tenha pedido durante o `dispatch`.
    fn route(&mut self, id: window::Id, msg: EngineMessage) -> Task<DaemonMessage> {
        // Controles de janela da titlebar custom (`window:drag`, `window:close`,
        // `window:resize:se`, …) são tratados AQUI, contra o `Id` da janela em
        // roteamento, e não dentro do motor — que, sem saber em qual janela vive,
        // teria de resolvê-lo via `window::latest()` e perderia o pointer-grab
        // serial no Wayland (ver `Runtime::main_id`). O `close` ainda passa por
        // `Runtime::close`, para o gancho `on_close` poder salvar a geometria.
        if let EngineMessage::UiClick(action) = &msg
            && let Some(cmd) = action.strip_prefix("window:")
        {
            return match cmd {
                "close" => self.close(id),
                _ => window_control(id, cmd),
            };
        }

        // 1. despacha ao motor da janela (borrow escopado)
        let ui_task = match self.windows.get_mut(&id) {
            Some(engine) => engine
                .dispatch(&msg)
                .map(move |m| DaemonMessage::Ui { id, msg: m }),
            None => return Task::none(),
        };

        // Observador de persistência: depois do dispatch (o interesse é o estado
        // resultante), e só na principal — é lá que vive o formulário cujo
        // estado o app quer guardar.
        if id == self.main_id
            && let (Some(hook), Some(engine)) = (&self.on_message, self.windows.get(&id))
        {
            hook(&msg, engine);
        }

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

        // 4. se o motor pediu para fechar a própria janela (`close_window()` na
        // Lua), fecha — pela mesma porta do botão da titlebar, para o gancho
        // `on_close` da principal também valer aqui.
        if self
            .windows
            .get_mut(&id)
            .map(|e| e.take_close_requested())
            .unwrap_or(false)
        {
            tasks.push(self.close(id));
        }

        Task::batch(tasks)
    }

    /// Materializa um [`WindowSpec`] numa janela nova: constrói um motor fresco,
    /// abre a janela (o `Id` vem síncrono) e registra motor + título.
    fn open_child(&mut self, spec: WindowSpec) -> Task<DaemonMessage> {
        let (w, h) = spec.size.unwrap_or((640.0, 480.0));
        let mut settings = window::Settings {
            size: Size::new(w, h),
            resizable: spec.resizable,
            ..window::Settings::default()
        };
        // O app tem a última palavra sobre a aparência da filha (ex.: também
        // borderless, num app com titlebar própria).
        if let Some(f) = &self.child_settings {
            f(&spec, &mut settings);
        }

        let WindowSpec {
            source,
            title,
            data,
            ..
        } = spec;
        let (engine, fallback_title) = build_engine(source, &data);
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
        self.titles
            .get(&id)
            .cloned()
            .unwrap_or_else(|| "Glacier".to_string())
    }

    fn theme(&self, id: window::Id) -> iced::Theme {
        self.windows
            .get(&id)
            .map(|e| e.theme())
            .unwrap_or(iced::Theme::Dark)
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
            // O pedido de fechar da WM (Alt+F4, botão da barra, logout) — chega
            // ANTES do fechamento, que é o único momento em que ainda dá para
            // consultar a geometria da janela para o gancho `on_close`. Só tem
            // efeito se a janela declarar `exit_on_close_request: false`.
            window::close_requests().map(DaemonMessage::CloseRequested),
            iced::time::every(self.reload_period)
                .map(|_| DaemonMessage::TickAll(EngineMessage::FileChanged(String::new()))),
            iced::time::every(self.toast_period)
                .map(|_| DaemonMessage::TickAll(EngineMessage::ToastTick)),
        ];

        // Eventos da bandeja (cliques de menu/ícone), só quando ela subiu. Drena
        // os canais globais do `tray-icon` (ver [`crate::tray::event_stream`]).
        if self.tray.is_some() {
            subs.push(iced::Subscription::run(crate::tray::event_stream).map(DaemonMessage::Tray));
        }

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

/// Traduz uma ação `window:<cmd>` da titlebar custom na `Task` do iced
/// correspondente, dirigida ao `Id` **conhecido** da janela — não ao que um
/// `window::latest()` devolveria depois. Ver [`Runtime::main_id`].
fn window_control(id: window::Id, cmd: &str) -> Task<DaemonMessage> {
    if let Some(dir) = cmd.strip_prefix("resize:") {
        return match resize_direction(dir) {
            Some(d) => window::drag_resize(id, d),
            None => Task::none(),
        };
    }
    match cmd {
        "minimize" => window::minimize(id, true),
        "maximize" | "toggle_maximize" => window::toggle_maximize(id),
        "drag" => window::drag(id),
        _ => Task::none(),
    }
}

/// Direção de um puxador de redimensionamento (`window:resize:se`, …). Aceita as
/// abreviações de bússola e os nomes por extenso.
fn resize_direction(s: &str) -> Option<window::Direction> {
    use window::Direction::*;
    Some(match s.trim().to_ascii_lowercase().as_str() {
        "n" | "north" | "top" => North,
        "s" | "south" | "bottom" => South,
        "e" | "east" | "right" => East,
        "w" | "west" | "left" => West,
        "ne" | "northeast" | "north-east" => NorthEast,
        "nw" | "northwest" | "north-west" => NorthWest,
        "se" | "southeast" | "south-east" => SouthEast,
        "sw" | "southwest" | "south-west" => SouthWest,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EngineMessage;
    use crate::component::{Component, Context, Template};

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
        let (engine, title) = build_engine(
            WindowSource::File("examples/janelas_glacier/detalhe.gv".into()),
            &[],
        );
        assert_eq!(title, "detalhe");
        // O motor da nova janela renderiza a tela carregada sem erro.
        assert!(engine.render_current().is_ok());
    }

    #[test]
    fn build_engine_semeia_data_no_contexto() {
        let (engine, _) = build_engine(
            WindowSource::File("examples/janelas_glacier/detalhe.gv".into()),
            &[
                ("url".into(), "http://x".into()),
                ("token".into(), "abc".into()),
            ],
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
        assert_eq!(
            b.get_data("rx").map(String::as_str),
            Some("ping:{\"v\":\"1\"}")
        );
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
