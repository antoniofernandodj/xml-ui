//! Componentes que encapsulam UI (template XML) + comportamento + estado próprio.
//!
//! Em vez de o app registrar a UI (`register_component`) e tratar o comportamento
//! à parte no seu `update()`, um [`Component`] junta os dois num único tipo que o
//! motor registra de uma vez via [`crate::GlacierUI::register`].

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

/// Um efeito assíncrono que um componente solicita durante o `update`.
///
/// O motor o transforma num [`iced::Task`]; quando o future completa, o
/// [`EffectOutcome`] resultante é aplicado (via
/// [`crate::EngineMessage::EffectOutcome`]): seus pares `(chave, valor)` são
/// mesclados no contexto, o toast (se houver) é exibido, e a UI é reavaliada. É
/// a peça que deixa um componente disparar I/O (rede, disco, timers) e refletir
/// o resultado no estado — e pedir um toast do resultado — sem bloquear a thread
/// de UI.
pub enum Effect {
    /// Executa um future e aplica o [`EffectOutcome`] resultante.
    Perform(Pin<Box<dyn Future<Output = EffectOutcome> + Send>>),
}

/// O que um efeito assíncrono pede ao motor ao terminar, além de dados — o
/// mesmo vocabulário que o [`Context`] já expõe para o código síncrono de
/// `update()`, só que aplicado depois que o `future` resolve (quando não há mais
/// um `Context` vivo para chamar `ctx.show_toast`).
///
/// Todo `ctx.perform` devolve um `EffectOutcome`. Use os construtores
/// [`EffectOutcome::data`] / [`EffectOutcome::toast`] e o builder
/// [`EffectOutcome::with_toast`] para montá-lo:
///
/// ```ignore
/// // só dados
/// EffectOutcome::data(vec![("status".into(), "ok".into())])
/// // dados + toast do resultado
/// EffectOutcome::data(vec![("saved".into(), "true".into())])
///     .with_toast(ToastSpec::success(msg))
/// // só toast
/// EffectOutcome::toast(ToastSpec::error(msg))
/// ```
#[derive(Debug, Clone, Default)]
pub struct EffectOutcome {
    /// Pares `(chave, valor)` mesclados no contexto (como um `ContextPatch`).
    pub patch: Vec<(String, String)>,
    /// Toast a exibir ao terminar, se houver. Diálogo/navegação ficam de fora
    /// desta fase — dá pra acrescentar depois com o mesmo mecanismo.
    pub toast: Option<crate::toasts::ToastSpec>,
}

impl EffectOutcome {
    /// Só dados: mescla estes pares `(chave, valor)` no contexto, sem toast.
    pub fn data(patch: Vec<(String, String)>) -> Self {
        Self { patch, toast: None }
    }

    /// Só um toast, sem dados — para um efeito cujo resultado é apenas a
    /// notificação.
    pub fn toast(spec: crate::toasts::ToastSpec) -> Self {
        Self { patch: Vec::new(), toast: Some(spec) }
    }

    /// Anexa (ou substitui) o toast deste outcome — encadeável sobre
    /// [`EffectOutcome::data`].
    pub fn with_toast(mut self, spec: crate::toasts::ToastSpec) -> Self {
        self.toast = Some(spec);
        self
    }
}

/// Uma requisição de rede pedida pela camada Lua via `fetch(url, opts)`
/// (ver [`crate::luau`]). É acumulada no [`Context`] durante o `update` e
/// convertida pelo motor num efeito assíncrono (HTTP via [`crate::net`]); ao
/// completar, a corrotina Lua suspensa `id` é retomada com o [`FetchResult`].
#[derive(Debug, Clone)]
pub struct PendingFetch {
    /// Identifica a corrotina suspensa que espera esta resposta.
    pub(crate) id: u64,
    pub(crate) url: String,
    pub(crate) method: String,
    pub(crate) body: Option<String>,
    pub(crate) headers: Vec<(String, String)>,
}

impl PendingFetch {
    pub(crate) fn new(
        id: u64,
        url: String,
        method: String,
        body: Option<String>,
        headers: Vec<(String, String)>,
    ) -> Self {
        Self { id, url, method, body, headers }
    }
}

/// Resultado de um `fetch`, entregue de volta à corrotina Lua como uma tabela
/// `{ ok, status, body, error }`.
#[derive(Debug, Clone)]
pub struct FetchResult {
    /// `true` se o status HTTP está em 2xx.
    pub ok: bool,
    /// Código de status HTTP (0 quando a requisição nem chegou a responder).
    pub status: u16,
    /// Corpo da resposta como texto.
    pub body: String,
    /// Mensagem de erro (vazia em caso de sucesso).
    pub error: String,
}

impl FetchResult {
    /// Um resultado de falha (sem resposta): `ok = false`, `status = 0`.
    pub fn error(msg: impl Into<String>) -> Self {
        Self { ok: false, status: 0, body: String::new(), error: msg.into() }
    }
}

/// Tipo de stream de vida longa aberto pela camada Lua: `sse` (só leitura,
/// Server-Sent Events sobre HTTP) ou `websocket` (bidirecional). Faz parte da
/// identidade da subscription do motor (ver [`crate::net::StreamKey`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamKind {
    /// Server-Sent Events: o motor lê o corpo HTTP evento a evento.
    Sse,
    /// WebSocket: leitura *e* envio (`conn:send`) sobre a mesma conexão.
    Ws,
}

/// Um stream de vida longa que a camada Lua pediu para abrir via `sse(url, ..)`
/// / `websocket(url, ..)`. Acumulado no [`Context`] durante o `update` e
/// convertido pelo motor numa `iced::Subscription` (ver [`crate::net`]); cada
/// evento recebido chama de volta o handler Lua registrado (`on_message`, …)
/// via [`Component::on_stream_event`]. Ao contrário de [`PendingFetch`], não é
/// one-shot: fica vivo emitindo eventos até fechar.
#[derive(Debug, Clone)]
pub struct StreamRequest {
    /// Identifica este stream dentro do componente dono (roteamento dos eventos
    /// e dos comandos de saída).
    pub(crate) id: u64,
    pub(crate) kind: StreamKind,
    pub(crate) url: String,
    pub(crate) headers: Vec<(String, String)>,
}

impl StreamRequest {
    pub(crate) fn new(
        id: u64,
        kind: StreamKind,
        url: String,
        headers: Vec<(String, String)>,
    ) -> Self {
        Self { id, kind, url, headers }
    }
}

/// Um temporizador de disparo único pedido pela camada Lua via
/// `after(ms, fn)` (ver [`crate::luau`]). Acumulado no [`Context`] durante o
/// `update` e convertido pelo motor num efeito assíncrono
/// (`tokio::time::sleep`); ao vencer, o handler Lua registrado sob `id` é
/// chamado via [`Component::resume_timer`] — mesmo espírito de
/// [`PendingFetch`], mas sem suspender a corrotina que o agendou (o script
/// que chama `after` continua no mesmo turno, como `sse`/`websocket`).
#[derive(Debug, Clone)]
pub struct PendingTimer {
    /// Identifica o handler Lua registrado que este temporizador dispara.
    pub(crate) id: u64,
    pub(crate) delay_ms: u64,
}

impl PendingTimer {
    pub(crate) fn new(id: u64, delay_ms: u64) -> Self {
        Self { id, delay_ms }
    }
}

/// Comando de saída para um stream já aberto, pedido pela camada Lua via
/// `conn:send(texto)` / `conn:close()`. Acumulado no [`Context`] e entregue
/// pelo motor à conexão viva (canal `mpsc` guardado quando o stream ficou
/// pronto).
#[derive(Debug, Clone)]
pub struct StreamCommand {
    pub(crate) id: u64,
    pub(crate) kind: StreamCommandKind,
    pub(crate) text: String,
}

impl StreamCommand {
    pub(crate) fn new(id: u64, kind: StreamCommandKind, text: String) -> Self {
        Self { id, kind, text }
    }
}

/// A ação de um [`StreamCommand`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamCommandKind {
    /// Envia `text` pela conexão (WebSocket). Sem efeito em SSE (só leitura).
    Send,
    /// Fecha a conexão.
    Close,
}

/// O que aconteceu num stream aberto pela camada Lua, entregue a
/// [`Component::on_stream_event`] para chamar o handler Lua correspondente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamEventKind {
    /// Conexão estabelecida (`on_open`).
    Open,
    /// Uma mensagem/evento chegou (`on_message`) — o texto vem em `data`.
    Message,
    /// Erro na conexão (`on_error`) — a mensagem vem em `data`.
    Error,
    /// Conexão encerrada (`on_close`).
    Closed,
}

/// De onde vem o XML de um componente.
pub enum Template {
    /// Caminho em disco — mantém o hot-reload do motor.
    File(String),
    /// XML embutido no binário.
    Inline(String),
}

/// Pedido de navegação feito por um componente, aplicado pelo motor depois.
pub enum Nav {
    To(String),
    Back,
}

/// Pedido de diálogo feito por um componente (via [`Context::show_dialog`] /
/// [`Context::close_dialog`]), aplicado pelo motor depois — mesmo padrão de
/// [`Nav`].
pub enum DialogAction {
    Show(crate::dialogs::DialogSpec),
    Close,
}

/// De onde vem a UI de uma janela nova pedida por [`Context::open_window`].
///
/// O motor de origem (que atendeu o `update`) usa isto para materializar um
/// **novo** [`crate::GlacierUI`] independente para a janela — cada janela tem
/// seu próprio motor, sem estado compartilhado com a de origem.
pub enum WindowSource {
    /// Uma instância Rust de [`Component`] já pronta, registrada como tela
    /// inicial da nova janela. Caminho da API Rust
    /// ([`Context::open_window_component`]).
    Component(Box<dyn Component>),
    /// Um arquivo de template (`.gv`/`.xml`/`.kdl`) carregado como tela inicial
    /// da nova janela. Caminho principal da API Lua (`open_window{ file = ... }`).
    File(String),
    /// O **nome** de um componente já registrado no motor de origem. Resolvido
    /// para o caminho do arquivo (via `registered_components`) na hora da
    /// drenagem, de modo que a nova janela o carregue do zero, isolada.
    Named(String),
}

/// Pedido de abrir uma nova janela, acumulado em [`Context`] durante o `update`
/// e aplicado pelo motor/daemon depois — mesmo padrão de [`Nav`]/[`DialogAction`].
///
/// Não deriva `Clone`/`Debug` porque [`WindowSource::Component`] carrega um
/// `Box<dyn Component>`; por isso o pedido nunca vira uma mensagem do iced —
/// vive no motor (`pending_windows`) até o daemon consumi-lo.
pub struct WindowSpec {
    /// A fonte da UI da nova janela.
    pub source: WindowSource,
    /// Título da janela (barra de título). `None` cai num default do daemon.
    pub title: Option<String>,
    /// Tamanho inicial `(largura, altura)` em px lógicos. `None` usa o default.
    pub size: Option<(f32, f32)>,
    /// Se a janela é redimensionável. Default `true`.
    pub resizable: bool,
}

impl WindowSpec {
    /// Nova janela mostrando uma instância Rust de [`Component`].
    pub fn component(comp: Box<dyn Component>) -> Self {
        Self::from_source(WindowSource::Component(comp))
    }

    /// Nova janela carregando um arquivo de template.
    pub fn file(path: impl Into<String>) -> Self {
        Self::from_source(WindowSource::File(path.into()))
    }

    /// Nova janela carregando um componente já registrado, pelo nome.
    pub fn named(name: impl Into<String>) -> Self {
        Self::from_source(WindowSource::Named(name.into()))
    }

    fn from_source(source: WindowSource) -> Self {
        Self { source, title: None, size: None, resizable: true }
    }

    /// Define o título da janela (encadeável).
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Define o tamanho inicial `(largura, altura)` (encadeável).
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = Some((width, height));
        self
    }

    /// Define se a janela é redimensionável (encadeável).
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }
}

/// Uma variável de contexto nomeada: agrupa a chave e o valor num único valor,
/// aplicado de uma vez com [`Context::set_var`]. Útil para declarar defaults de
/// forma legível em vez de repetir a chave string solta.
pub struct ContextVar {
    key: String,
    value: String,
}

impl ContextVar {
    /// Cria uma variável com sua chave e valor.
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self { key: key.into(), value: value.into() }
    }

    /// A chave (nome) da variável.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// O valor da variável.
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Acesso restrito ao estado do motor entregue ao componente durante
/// `init`/`update`. Expõe só o necessário (ler/escrever dados e pedir
/// navegação), evitando o conflito de borrow que existiria ao passar o
/// `GlacierUI` inteiro.
pub struct Context<'a> {
    pub(crate) data: &'a mut HashMap<String, String>,
    pub(crate) nav: Option<Nav>,
    pub(crate) effects: Vec<Effect>,
    pub(crate) dialog: Option<DialogAction>,
    pub(crate) toasts: Vec<crate::toasts::ToastSpec>,
    /// Requisições de rede pedidas via `fetch` na camada Lua, transformadas em
    /// efeitos assíncronos pelo motor após o `update`.
    pub(crate) fetches: Vec<PendingFetch>,
    /// Streams de vida longa (`sse`/`websocket`) que a camada Lua pediu para
    /// abrir; o motor os converte em subscriptions após o `update`.
    pub(crate) streams: Vec<StreamRequest>,
    /// Comandos de saída (`conn:send`/`conn:close`) para streams já abertos.
    pub(crate) stream_cmds: Vec<StreamCommand>,
    /// Temporizadores (`after`) pedidos pela camada Lua; o motor os agenda
    /// como efeitos assíncronos após o `update`.
    pub(crate) timers: Vec<PendingTimer>,
    /// Novas janelas pedidas via [`Context::open_window`]; o motor as drena para
    /// `pending_windows` e o daemon as abre após o `update`.
    pub(crate) windows: Vec<WindowSpec>,
    /// Viewport atual `(largura, altura)` em px lógicos, lido por
    /// `Context::viewport` (a camada Lua expõe isto via `viewport()`). Só o
    /// motor escreve aqui (ver [`Context::set_viewport`]); um componente lê.
    pub(crate) viewport: (f32, f32),
}

impl<'a> Context<'a> {
    /// Cria um contexto novo espelhando `data`, com todos os acumuladores
    /// (efeitos, fetches, streams, …) vazios. Ponto único de construção para
    /// que adicionar um acumulador não force editar cada call-site.
    pub(crate) fn new(data: &'a mut HashMap<String, String>) -> Self {
        Self {
            data,
            nav: None,
            effects: Vec::new(),
            dialog: None,
            toasts: Vec::new(),
            fetches: Vec::new(),
            streams: Vec::new(),
            stream_cmds: Vec::new(),
            timers: Vec::new(),
            windows: Vec::new(),
            viewport: (0.0, 0.0),
        }
    }

    /// Define o viewport atual `(largura, altura)`, lido por scripts Luau via
    /// `viewport()`. Chamado pelo motor antes de rodar o componente — não é
    /// para uso de código de `update()`.
    pub(crate) fn set_viewport(&mut self, vp: (f32, f32)) {
        self.viewport = vp;
    }

    /// Lê um valor do contexto de estado.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }

    /// Define/atualiza um valor do contexto de estado (visível aos templates).
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.data.insert(key.to_string(), value.into());
    }

    /// Aplica uma [`ContextVar`] (chave + valor) ao contexto.
    pub fn set_var(&mut self, var: &ContextVar) {
        self.data.insert(var.key.clone(), var.value.clone());
    }

    /// Pede ao motor para navegar para outra tela após o `update`.
    pub fn navigate_to(&mut self, screen: &str) {
        self.nav = Some(Nav::To(screen.to_string()));
    }

    /// Pede ao motor para voltar à tela anterior após o `update`.
    pub fn navigate_back(&mut self) {
        self.nav = Some(Nav::Back);
    }

    /// Pede ao motor para exibir um diálogo modal (ver [`crate::dialogs`])
    /// sobreposto à tela atual após o `update`. Substitui qualquer diálogo já
    /// em exibição.
    pub fn show_dialog(&mut self, spec: crate::dialogs::DialogSpec) {
        self.dialog = Some(DialogAction::Show(spec));
    }

    /// Pede ao motor para fechar o diálogo em exibição (se houver) após o
    /// `update`, sem despachar nenhuma ação de botão.
    pub fn close_dialog(&mut self) {
        self.dialog = Some(DialogAction::Close);
    }

    /// Pede ao motor/daemon para abrir uma **nova janela** após o `update`,
    /// descrita por [`WindowSpec`]. A janela é atendida por um [`crate::GlacierUI`]
    /// novo e independente (contexto/estado isolados da janela de origem).
    ///
    /// ```ignore
    /// // arquivo de template numa janela de 400x300 intitulada "Detalhe"
    /// ctx.open_window(WindowSpec::file("telas/detalhe.gv").title("Detalhe").size(400.0, 300.0));
    /// ```
    pub fn open_window(&mut self, spec: WindowSpec) {
        self.windows.push(spec);
    }

    /// Atalho de [`Context::open_window`] para abrir uma janela mostrando uma
    /// instância Rust de [`Component`] — o caminho da API Rust.
    pub fn open_window_component(&mut self, comp: Box<dyn Component>) {
        self.windows.push(WindowSpec::component(comp));
    }

    /// Pede ao motor para mostrar um toast (ver [`crate::toasts`]) após o
    /// `update`. Ao contrário de [`Context::show_dialog`], é cumulativo — não
    /// substitui nenhum toast já em exibição, e pode ser chamado mais de uma
    /// vez no mesmo `update` para empilhar vários.
    pub fn show_toast(&mut self, spec: crate::toasts::ToastSpec) {
        self.toasts.push(spec);
    }

    /// Agenda um efeito assíncrono: o `future` roda no executor do `iced` e, ao
    /// completar, seu [`EffectOutcome`] é aplicado (dados mesclados no contexto,
    /// toast exibido se houver) e a UI é reavaliada. Use para rede, disco e
    /// qualquer I/O sem bloquear a UI.
    ///
    /// O `future` devolve um [`EffectOutcome`] — dados a mesclar e, opcionalmente,
    /// um toast do resultado. Monte-o com [`EffectOutcome::data`] /
    /// [`EffectOutcome::toast`] / [`EffectOutcome::with_toast`].
    ///
    /// ```ignore
    /// fn update(&mut self, action: &str, _v: Option<&str>, ctx: &mut Context) {
    ///     if action == "load" {
    ///         // só dados
    ///         ctx.perform(async {
    ///             let body = fetch().await;
    ///             EffectOutcome::data(vec![
    ///                 ("status".into(), "ok".into()),
    ///                 ("body".into(), body),
    ///             ])
    ///         });
    ///     }
    ///     if action == "save" {
    ///         // dados + toast do resultado
    ///         ctx.perform(async {
    ///             let msg = save().await;
    ///             EffectOutcome::data(vec![("saved".into(), "true".into())])
    ///                 .with_toast(ToastSpec::success(msg))
    ///         });
    ///     }
    /// }
    /// ```
    pub fn perform<F>(&mut self, future: F)
    where
        F: Future<Output = EffectOutcome> + Send + 'static,
    {
        self.effects.push(Effect::Perform(Box::pin(future)));
    }

    /// Agenda um efeito que produz um único par `(chave, valor)`.
    pub fn perform_one<F>(&mut self, future: F)
    where
        F: Future<Output = (String, String)> + Send + 'static,
    {
        self.effects.push(Effect::Perform(Box::pin(async move {
            EffectOutcome::data(vec![future.await])
        })));
    }
}

/// Encapsula a UI, o comportamento e o estado próprio de um componente.
pub trait Component {
    /// Nome único, usado para registrar o template e rotear as ações.
    fn name(&self) -> &str;

    /// A UI deste componente.
    fn template(&self) -> Template;

    /// Semeia o contexto com o estado inicial (opcional).
    fn init(&mut self, _ctx: &mut Context) {}

    /// Sub-componentes que este componente possui. Ao registrar o pai, o motor
    /// registra cada filho em cascata (template + `init`), e as ações vindas da
    /// UI de um filho (referenciado por `<Component name="...">`) são roteadas
    /// para o `update` do próprio filho.
    ///
    /// Padrão: sem filhos.
    fn children(&self) -> Vec<Box<dyn Component>> {
        Vec::new()
    }

    /// Reage a uma ação vinda da sua própria UI.
    ///
    /// `value` vem preenchido em inputs (`UiInputChanged`); é `None` em
    /// cliques (`UiClick`).
    fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context);

    /// Reage ao `onSubmit` de um `<Form>` (veja [`crate::forms::Form`]). Ao
    /// contrário de `update` — que recebe todo o resto (cliques, `onChange`,
    /// drag-and-drop, ...) — Enter num `formControl` ou um botão de submit
    /// dentro de um `<Form>` chegam aqui, não em `update`: a atualização de
    /// cada campo e a submissão do formulário nunca competem pelo mesmo
    /// `match`. `action` é a string do `onSubmit` (já sem o namespace do
    /// componente). Padrão: no-op — componentes sem formulário não precisam
    /// implementar. Um jeito comum de implementar é só delegar pra closure
    /// registrada via `FormBuilder::on_submit`:
    /// ```ignore
    /// fn on_form_submit(&mut self, _action: &str, ctx: &mut Context) {
    ///     self.form.submit(ctx);
    /// }
    /// ```
    fn on_form_submit(&mut self, _action: &str, _ctx: &mut Context) {}

    /// Retoma um `fetch` assíncrono que completou: o motor entrega o `id` da
    /// requisição (ver [`PendingFetch`]) e o [`FetchResult`]. Componentes que
    /// não fazem rede não precisam implementar. A [`crate::luau::LuauComponent`]
    /// usa isto para retomar a corrotina Lua suspensa no ponto do `fetch`,
    /// passando o resultado — o que dá a aparência de `async/await`.
    fn resume_fetch(&mut self, _id: u64, _result: &FetchResult, _ctx: &mut Context) {}

    /// Entrega um evento de um stream de vida longa (`sse`/`websocket`) aberto
    /// pelo componente: o `id` da requisição (ver [`StreamRequest`]), o que
    /// aconteceu ([`StreamEventKind`]) e, para `Message`/`Error`, o texto em
    /// `data` (vazio para `Open`/`Closed`). Componentes sem streams não
    /// precisam implementar. A [`crate::luau::LuauComponent`] usa isto para chamar
    /// o handler Lua registrado (`on_message`, `on_open`, `on_error`,
    /// `on_close`), que pode escrever em `ctx` como qualquer ação.
    fn on_stream_event(
        &mut self,
        _id: u64,
        _kind: StreamEventKind,
        _data: &str,
        _ctx: &mut Context,
    ) {
    }

    /// Dispara o handler de um temporizador (`after(ms, fn)`, ver
    /// [`PendingTimer`]) cujo prazo venceu: o motor entrega o `id` do
    /// temporizador. Componentes sem timers não precisam implementar. A
    /// [`crate::luau::LuauComponent`] usa isto para chamar o handler Lua
    /// registrado, exatamente como um evento de stream.
    fn resume_timer(&mut self, _id: u64, _ctx: &mut Context) {}

    /// Fontes contínuas de eventos externos (sockets, timers, watchers) que
    /// alimentam o contexto. Mapeie cada stream para
    /// [`crate::EngineMessage::ContextPatch`] e o motor mesclará os pares no
    /// contexto e reavaliará a UI a cada item. O motor agrega as subscriptions
    /// de todos os componentes registrados em [`crate::GlacierUI::subscription`].
    ///
    /// Padrão: nenhuma subscription.
    fn subscription(&self) -> iced::Subscription<crate::EngineMessage> {
        iced::Subscription::none()
    }
}
