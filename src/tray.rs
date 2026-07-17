//! Ícone de **bandeja** (system tray / área de notificação) para o runner
//! [`crate::GlacierDaemon`].
//!
//! O ponto é permitir um app que **sobrevive à última janela**: quando o
//! usuário fecha a janela, o app recolhe para a bandeja em vez de encerrar, e o
//! ícone controla o ciclo de vida (reabrir, sair) e um interruptor global de
//! notificações do SO (ver [`set_notifications_enabled`]).
//!
//! ## Por que uma thread dedicada
//!
//! A bandeja (crate `tray-icon`) precisa de um **loop de eventos do SO** na
//! thread que a cria — e o `iced`/`winit` é dono do loop da thread principal,
//! sem um gancho para injetarmos a bandeja nele. Então a bandeja sobe numa
//! **thread própria** que roda o loop da plataforma:
//!
//! - **Linux**: `libappindicator` + GTK. A thread faz `gtk::init()` e roda o
//!   loop GTK; as atualizações de menu (rótulo/checkbox) acontecem nessa mesma
//!   thread (os itens do menu são `!Send`).
//! - **Windows**: `tray-icon` cria uma janela oculta e depende do
//!   *message-loop* Win32 da thread que a criou — a thread bombeia mensagens
//!   com `PeekMessage`/`DispatchMessage`.
//! - **macOS**: a bandeja exige a **thread principal** — a thread dedicada não
//!   serve. Fica como **limitação conhecida**: [`spawn`] devolve `None`, e o app
//!   volta a encerrar na última janela (ver [`crate::daemon`]).
//!
//! Os **cliques** (menu e ícone) chegam de volta pelos canais globais do
//! `tray-icon` (`MenuEvent::receiver()` / `TrayIconEvent::receiver()`), drenados
//! por uma subscription do daemon ([`event_stream`]). Os **comandos** para a
//! bandeja (mudar rótulo/checkbox) vão pelo [`TrayHandle`] até a thread.
//!
//! Todo o uso concreto de `tray-icon`/`gtk` fica atrás da feature **`tray`**;
//! sem ela, os tipos de dados aqui continuam existindo, [`spawn`] é um no-op que
//! devolve `None` e nenhum `gtk` é arrastado para o build.

use std::sync::atomic::{AtomicBool, Ordering};

/// Interruptor **global de processo** das notificações nativas do SO emitidas
/// por `notify()` (ver `crate::emit_os_notification`). Começa ligado.
///
/// É um estado de processo de propósito: o item "Disable/Enable notifications"
/// da bandeja o alterna sem precisar atravessar a camada Luau — o `notify()`
/// simplesmente o consulta antes de emitir.
static NOTIFICATIONS_ENABLED: AtomicBool = AtomicBool::new(true);

/// `true` se as notificações do SO estão habilitadas (o default). Consultado por
/// `crate::emit_os_notification` antes de disparar.
pub fn notifications_enabled() -> bool {
    NOTIFICATIONS_ENABLED.load(Ordering::Relaxed)
}

/// Liga/desliga as notificações do SO em todo o processo. É o que o gancho
/// `on_tray` chama ao tratar o item de menu de notificações.
pub fn set_notifications_enabled(enabled: bool) {
    NOTIFICATIONS_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Um item do menu da bandeja. Ver [`TrayConfig`].
#[derive(Debug, Clone)]
pub enum TrayItem {
    /// Botão comum: dispara o gancho `on_tray` com este `id`.
    Button { id: String, label: String },
    /// Item marcável (com checkbox). O estado visual é controlado pelo app via
    /// [`TrayActions::set_checked`].
    Check {
        id: String,
        label: String,
        checked: bool,
    },
    /// Linha separadora (sem id, não clicável).
    Separator,
}

impl TrayItem {
    /// Botão comum com `id` (o que chega ao `on_tray`) e rótulo visível.
    pub fn button(id: impl Into<String>, label: impl Into<String>) -> Self {
        TrayItem::Button {
            id: id.into(),
            label: label.into(),
        }
    }

    /// Item marcável, começando em `checked`.
    pub fn check(id: impl Into<String>, label: impl Into<String>, checked: bool) -> Self {
        TrayItem::Check {
            id: id.into(),
            label: label.into(),
            checked,
        }
    }

    /// Separador.
    pub fn separator() -> Self {
        TrayItem::Separator
    }
}

/// Configuração da bandeja passada a [`crate::GlacierDaemon::tray`].
#[derive(Debug, Clone)]
pub struct TrayConfig {
    /// Bytes de uma imagem (PNG, …) para o ícone — decodificada para RGBA via o
    /// crate `image` que a glacier já usa.
    pub icon: Vec<u8>,
    /// Texto do tooltip do ícone.
    pub tooltip: String,
    /// Itens do menu, de cima para baixo.
    pub items: Vec<TrayItem>,
}

/// Mensagem vinda da bandeja para o daemon (ver [`event_stream`]).
#[derive(Debug, Clone)]
pub enum TrayMsg {
    /// Um item de menu com este `id` foi clicado.
    Menu(String),
    /// Clique **esquerdo** no ícone (Windows). No Linux não é emitido — o
    /// `tray-icon` não entrega evento de clique no ícone; lá o clique esquerdo
    /// abre o próprio menu (ver `with_menu_on_left_click`).
    IconLeftClick,
}

/// O que o gancho `on_tray` pediu que o daemon faça. Interpretado em
/// `crate::daemon::Runtime`.
#[derive(Debug, Clone, Copy)]
pub enum TrayRequest {
    /// Reabrir/focar a janela principal.
    OpenMain,
    /// Encerrar o app de vez.
    Quit,
}

/// Comando enviado **para** a thread da bandeja (atualizar o menu). Sem a
/// feature `tray` não há thread que os leia (o `apply` é feature-gated) e o
/// `TrayHandle` nunca é criado (`spawn` devolve `None`), então os campos ficam
/// legitimamente sem leitor — daí o `allow(dead_code)` só nesse caso.
#[cfg_attr(not(feature = "tray"), allow(dead_code))]
enum TrayCommand {
    /// Trocar o rótulo do item `id`.
    SetLabel(String, String),
    /// Marcar/desmarcar o item `id` (só itens `Check`).
    SetChecked(String, bool),
    /// Derrubar a bandeja e encerrar a thread.
    Shutdown,
}

/// Alça para falar com a thread da bandeja. Guardada pelo `Runtime` do daemon e
/// entregue ao gancho `on_tray` via [`TrayActions`].
pub struct TrayHandle {
    tx: std::sync::mpsc::Sender<TrayCommand>,
}

impl TrayHandle {
    fn send(&self, cmd: TrayCommand) {
        // A thread morre junto com o processo; um erro aqui só significa que ela
        // já saiu — nada a fazer.
        let _ = self.tx.send(cmd);
    }

    /// Pede o encerramento da bandeja (a thread derruba o ícone e sai). Chamado
    /// pelo daemon ao sair.
    pub fn shutdown(&self) {
        self.send(TrayCommand::Shutdown);
    }
}

/// Ações que o gancho `on_tray` pode disparar. Métodos que dependem do runner
/// (abrir a principal, sair) só registram a intenção em `request`, que o daemon
/// lê e traduz numa `Task`; os de menu (rótulo/checkbox) vão direto à thread da
/// bandeja pelo [`TrayHandle`].
pub struct TrayActions<'a> {
    handle: &'a TrayHandle,
    pub(crate) request: Option<TrayRequest>,
}

impl<'a> TrayActions<'a> {
    /// Constrói o coletor de ações para uma rodada do gancho. Uso interno do
    /// daemon.
    pub(crate) fn new(handle: &'a TrayHandle) -> Self {
        Self {
            handle,
            request: None,
        }
    }

    /// Reabrir (ou focar, se já aberta) a janela principal.
    pub fn open_main(&mut self) {
        self.request = Some(TrayRequest::OpenMain);
    }

    /// Encerrar o app.
    pub fn quit(&mut self) {
        self.request = Some(TrayRequest::Quit);
    }

    /// Trocar o rótulo de um item de menu (ex.: alternar
    /// "Disable notifications" ↔ "Enable notifications").
    pub fn set_label(&self, id: &str, text: impl Into<String>) {
        self.handle
            .send(TrayCommand::SetLabel(id.to_string(), text.into()));
    }

    /// Marcar/desmarcar um item `Check`.
    pub fn set_checked(&self, id: &str, checked: bool) {
        self.handle
            .send(TrayCommand::SetChecked(id.to_string(), checked));
    }
}

// ───────────────────────── implementação com a feature `tray` ────────────────

/// Sobe a bandeja numa thread dedicada e devolve a alça para falar com ela.
/// Devolve `None` se não houver suporte (macOS, ou falha ao criar a thread/o
/// ícone) — nesse caso o daemon segue sem bandeja (encerra na última janela).
#[cfg(all(feature = "tray", any(target_os = "linux", target_os = "windows")))]
pub fn spawn(config: TrayConfig) -> Option<TrayHandle> {
    let (tx, rx) = std::sync::mpsc::channel::<TrayCommand>();
    let spawned = std::thread::Builder::new()
        .name("glacier-tray".to_string())
        .spawn(move || tray_thread(config, rx));
    match spawned {
        Ok(_) => Some(TrayHandle { tx }),
        Err(e) => {
            eprintln!("tray: falha ao criar a thread da bandeja: {e}");
            None
        }
    }
}

/// macOS e builds sem a feature `tray`: sem bandeja. O daemon volta a encerrar
/// na última janela.
#[cfg(not(all(feature = "tray", any(target_os = "linux", target_os = "windows"))))]
pub fn spawn(_config: TrayConfig) -> Option<TrayHandle> {
    #[cfg(target_os = "macos")]
    eprintln!("tray: bandeja não suportada no macOS (requer a thread principal)");
    None
}

/// Stream que drena os canais globais de evento do `tray-icon` e emite
/// [`TrayMsg`]. Registrada como subscription do daemon quando há bandeja. É um
/// `fn` (não closure) para `Subscription::run` derivar a chave do seu tipo.
///
/// Faz *polling* (não bloqueante) dos receivers `crossbeam` do `tray-icon`,
/// que são globais e síncronos — a ponte para o mundo async do iced é este laço
/// com `sleep`. O intervalo é curto o bastante para o clique parecer imediato e
/// longo o bastante para não acordar o loop à toa.
#[cfg(feature = "tray")]
pub fn event_stream() -> impl futures::Stream<Item = TrayMsg> {
    use futures::SinkExt;
    use std::time::Duration;
    use tray_icon::menu::MenuEvent;
    use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

    iced::stream::channel(64, |mut output: futures::channel::mpsc::Sender<TrayMsg>| async move {
        loop {
            while let Ok(ev) = MenuEvent::receiver().try_recv() {
                let _ = output.send(TrayMsg::Menu(ev.id.0)).await;
            }
            while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = ev
                {
                    let _ = output.send(TrayMsg::IconLeftClick).await;
                }
            }
            tokio::time::sleep(Duration::from_millis(120)).await;
        }
    })
}

/// Sem a feature `tray`: um stream que nunca emite. Só existe para o daemon
/// compilar (o ramo que o usa é morto, pois `tray` é sempre `None`).
#[cfg(not(feature = "tray"))]
pub fn event_stream() -> impl futures::Stream<Item = TrayMsg> {
    futures::stream::pending()
}

/// Corpo da thread da bandeja: inicializa a plataforma, constrói o ícone/menu e
/// roda o loop de eventos até o `Shutdown`.
#[cfg(all(feature = "tray", any(target_os = "linux", target_os = "windows")))]
fn tray_thread(config: TrayConfig, rx: std::sync::mpsc::Receiver<TrayCommand>) {
    #[cfg(target_os = "linux")]
    if let Err(e) = gtk::init() {
        eprintln!("tray: gtk::init falhou ({e}); bandeja indisponível");
        return;
    }

    let built = match build_tray(&config) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("tray: {e}");
            return;
        }
    };
    run_loop(built, rx);
}

/// A bandeja viva na thread: o `TrayIcon` (mantido para não ser destruído) e as
/// alças dos itens mutáveis, por id, para aplicar `SetLabel`/`SetChecked`.
#[cfg(all(feature = "tray", any(target_os = "linux", target_os = "windows")))]
struct BuiltTray {
    _tray: tray_icon::TrayIcon,
    items: std::collections::HashMap<String, ItemHandle>,
}

/// Alça de um item mutável do menu.
#[cfg(all(feature = "tray", any(target_os = "linux", target_os = "windows")))]
enum ItemHandle {
    Button(tray_icon::menu::MenuItem),
    Check(tray_icon::menu::CheckMenuItem),
}

#[cfg(all(feature = "tray", any(target_os = "linux", target_os = "windows")))]
impl ItemHandle {
    fn set_label(&self, text: &str) {
        match self {
            ItemHandle::Button(m) => m.set_text(text),
            ItemHandle::Check(c) => c.set_text(text),
        }
    }
    fn set_checked(&self, checked: bool) {
        if let ItemHandle::Check(c) = self {
            c.set_checked(checked);
        }
    }
}

/// Constrói o menu e o ícone a partir da [`TrayConfig`]. Roda na thread da
/// bandeja (os itens do `muda` são `!Send`).
#[cfg(all(feature = "tray", any(target_os = "linux", target_os = "windows")))]
fn build_tray(config: &TrayConfig) -> Result<BuiltTray, String> {
    use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIconBuilder};

    // PNG (ou outra imagem) → RGBA cru, via o crate `image` da glacier.
    let img = image::load_from_memory(&config.icon)
        .map_err(|e| format!("ícone inválido: {e}"))?
        .into_rgba8();
    let (w, h) = img.dimensions();
    let icon =
        Icon::from_rgba(img.into_raw(), w, h).map_err(|e| format!("ícone inválido: {e}"))?;

    let menu = Menu::new();
    let mut items = std::collections::HashMap::new();
    for item in &config.items {
        match item {
            TrayItem::Button { id, label } => {
                let mi = MenuItem::with_id(id.clone(), label, true, None);
                if let Err(e) = menu.append(&mi) {
                    eprintln!("tray: falha ao anexar item '{id}': {e}");
                }
                items.insert(id.clone(), ItemHandle::Button(mi));
            }
            TrayItem::Check {
                id,
                label,
                checked,
            } => {
                let ci = CheckMenuItem::with_id(id.clone(), label, true, *checked, None);
                if let Err(e) = menu.append(&ci) {
                    eprintln!("tray: falha ao anexar item '{id}': {e}");
                }
                items.insert(id.clone(), ItemHandle::Check(ci));
            }
            TrayItem::Separator => {
                if let Err(e) = menu.append(&PredefinedMenuItem::separator()) {
                    eprintln!("tray: falha ao anexar separador: {e}");
                }
            }
        }
    }

    let tray = TrayIconBuilder::new()
        .with_tooltip(&config.tooltip)
        .with_icon(icon)
        .with_menu(Box::new(menu))
        // No Linux não há evento de clique no ícone: o clique esquerdo tem de
        // abrir o próprio menu (senão o ícone fica inerte). No Windows o clique
        // esquerdo vira `TrayIconEvent::Click` (→ abrir a principal), então lá o
        // esquerdo NÃO abre o menu — o direito abre (default).
        .with_menu_on_left_click(cfg!(target_os = "linux"))
        .build()
        .map_err(|e| format!("falha ao criar o ícone da bandeja: {e}"))?;

    Ok(BuiltTray {
        _tray: tray,
        items,
    })
}

/// Aplica um comando de menu recebido do daemon. Compartilhado entre as
/// plataformas.
#[cfg(all(feature = "tray", any(target_os = "linux", target_os = "windows")))]
fn apply(built: &BuiltTray, cmd: TrayCommand) -> bool {
    match cmd {
        TrayCommand::SetLabel(id, text) => {
            if let Some(it) = built.items.get(&id) {
                it.set_label(&text);
            }
            false
        }
        TrayCommand::SetChecked(id, checked) => {
            if let Some(it) = built.items.get(&id) {
                it.set_checked(checked);
            }
            false
        }
        TrayCommand::Shutdown => true,
    }
}

/// Loop de eventos da bandeja no **Linux**: roda o loop GTK e, num `timeout`
/// periódico, drena os comandos do daemon. `Shutdown` chama `gtk::main_quit`.
#[cfg(all(feature = "tray", target_os = "linux"))]
fn run_loop(built: BuiltTray, rx: std::sync::mpsc::Receiver<TrayCommand>) {
    use std::rc::Rc;
    use std::time::Duration;

    let built = Rc::new(built);
    gtk::glib::timeout_add_local(Duration::from_millis(80), move || {
        loop {
            match rx.try_recv() {
                Ok(cmd) => {
                    if apply(&built, cmd) {
                        gtk::main_quit();
                        return gtk::glib::ControlFlow::Break;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    gtk::main_quit();
                    return gtk::glib::ControlFlow::Break;
                }
            }
        }
        gtk::glib::ControlFlow::Continue
    });
    gtk::main();
}

/// Loop de eventos da bandeja no **Windows**: bombeia as mensagens Win32 da
/// janela oculta que o `tray-icon` criou nesta thread e, entre elas, drena os
/// comandos do daemon.
///
/// NOTA: caminho não compilável/verificável nesta máquina Linux — validar num
/// build Windows real.
#[cfg(all(feature = "tray", target_os = "windows"))]
fn run_loop(built: BuiltTray, rx: std::sync::mpsc::Receiver<TrayCommand>) {
    use std::time::Duration;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        // Drena a fila de mensagens da janela oculta do tray (não bloqueante).
        unsafe {
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        // Drena os comandos do daemon.
        loop {
            match rx.try_recv() {
                Ok(cmd) => {
                    if apply(&built, cmd) {
                        return;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            }
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notificacoes_comecam_ligadas_e_alternam() {
        // Default de processo: ligado.
        assert!(notifications_enabled());
        set_notifications_enabled(false);
        assert!(!notifications_enabled());
        set_notifications_enabled(true);
        assert!(notifications_enabled());
    }

    #[test]
    fn tray_item_construtores() {
        match TrayItem::button("open", "Open") {
            TrayItem::Button { id, label } => {
                assert_eq!(id, "open");
                assert_eq!(label, "Open");
            }
            _ => panic!("esperava Button"),
        }
        match TrayItem::check("notif", "Notificações", true) {
            TrayItem::Check {
                id,
                label,
                checked,
            } => {
                assert_eq!(id, "notif");
                assert_eq!(label, "Notificações");
                assert!(checked);
            }
            _ => panic!("esperava Check"),
        }
        assert!(matches!(TrayItem::separator(), TrayItem::Separator));
    }

    #[test]
    fn tray_actions_registra_request_e_envia_comandos() {
        // Um handle de teste com o outro lado do canal em mãos, para inspecionar
        // os comandos que `set_label`/`set_checked` enfileiram.
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = TrayHandle { tx };

        let mut actions = TrayActions::new(&handle);
        assert!(actions.request.is_none());

        actions.open_main();
        assert!(matches!(actions.request, Some(TrayRequest::OpenMain)));
        actions.quit();
        assert!(matches!(actions.request, Some(TrayRequest::Quit)));

        actions.set_label("notifications", "Enable notifications");
        actions.set_checked("notifications", false);
        match rx.try_recv() {
            Ok(TrayCommand::SetLabel(id, text)) => {
                assert_eq!(id, "notifications");
                assert_eq!(text, "Enable notifications");
            }
            Ok(_) => panic!("esperava SetLabel, veio outro comando"),
            Err(e) => panic!("esperava SetLabel, canal vazio/fechado: {e}"),
        }
        assert!(matches!(
            rx.try_recv(),
            Ok(TrayCommand::SetChecked(id, false)) if id == "notifications"
        ));
    }
}
