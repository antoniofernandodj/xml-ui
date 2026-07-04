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
/// O caso comum (só dados, sem toast) continua ergonômico: qualquer future que
/// devolva `Vec<(String, String)>` vira um `EffectOutcome` automaticamente (ver
/// o `From` abaixo), então [`Context::perform`] segue aceitando o retorno
/// antigo sem mudança.
///
/// ```ignore
/// ctx.perform(async move {
///     let msg = run_command().await;
///     EffectOutcome { patch: vec![("status".into(), "ok".into())],
///                     toast: Some(ToastSpec::success(msg)) }
/// });
/// ```
#[derive(Debug, Clone, Default)]
pub struct EffectOutcome {
    /// Pares `(chave, valor)` mesclados no contexto (como um `ContextPatch`).
    pub patch: Vec<(String, String)>,
    /// Toast a exibir ao terminar, se houver. Diálogo/navegação ficam de fora
    /// desta fase — dá pra acrescentar depois com o mesmo mecanismo.
    pub toast: Option<crate::toasts::ToastSpec>,
}

/// Só dados, sem toast — preserva a compatibilidade de `ctx.perform(async {
/// vec![...] })`, que continua compilando sem mudar uma linha.
impl From<Vec<(String, String)>> for EffectOutcome {
    fn from(patch: Vec<(String, String)>) -> Self {
        Self { patch, toast: None }
    }
}

/// Um único par `(chave, valor)`, sem toast — conveniência para efeitos que
/// produzem só um dado.
impl From<(String, String)> for EffectOutcome {
    fn from(pair: (String, String)) -> Self {
        Self { patch: vec![pair], toast: None }
    }
}

/// Só um toast, sem dados — para um efeito cujo resultado é apenas a notificação.
impl From<crate::toasts::ToastSpec> for EffectOutcome {
    fn from(toast: crate::toasts::ToastSpec) -> Self {
        Self { patch: Vec::new(), toast: Some(toast) }
    }
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
}

impl<'a> Context<'a> {
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
    /// O `future` pode devolver qualquer coisa que vire um [`EffectOutcome`]:
    /// `Vec<(String, String)>` (só dados — o caso comum), `(String, String)`,
    /// uma [`crate::toasts::ToastSpec`] (só toast), ou um `EffectOutcome`
    /// completo. Assim o código que só mescla dados não muda, e quem quer
    /// notificar o resultado devolve o toast direto — sem chaves reservadas.
    ///
    /// ```ignore
    /// fn update(&mut self, action: &str, _v: Option<&str>, ctx: &mut Context) {
    ///     if action == "load" {
    ///         // só dados
    ///         ctx.perform(async {
    ///             let body = fetch().await;
    ///             vec![("status".into(), "ok".into()), ("body".into(), body)]
    ///         });
    ///     }
    ///     if action == "save" {
    ///         // dados + toast do resultado
    ///         ctx.perform(async {
    ///             let msg = save().await;
    ///             EffectOutcome { patch: vec![("saved".into(), "true".into())],
    ///                             toast: Some(ToastSpec::success(msg)) }
    ///         });
    ///     }
    /// }
    /// ```
    pub fn perform<F, T>(&mut self, future: F)
    where
        F: Future<Output = T> + Send + 'static,
        T: Into<EffectOutcome> + Send + 'static,
    {
        self.effects.push(Effect::Perform(Box::pin(async move {
            future.await.into()
        })));
    }

    /// Agenda um efeito que produz um único par `(chave, valor)`.
    pub fn perform_one<F>(&mut self, future: F)
    where
        F: Future<Output = (String, String)> + Send + 'static,
    {
        self.effects.push(Effect::Perform(Box::pin(async move {
            EffectOutcome::from(future.await)
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
