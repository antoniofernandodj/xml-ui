//! Componentes que encapsulam UI (template XML) + comportamento + estado próprio.
//!
//! Em vez de o app registrar a UI (`register_component`) e tratar o comportamento
//! à parte no seu `update()`, um [`Component`] junta os dois num único tipo que o
//! motor registra de uma vez via [`crate::UiEngine::register`].

use std::collections::HashMap;

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
/// `UiEngine` inteiro.
pub struct Context<'a> {
    pub(crate) data: &'a mut HashMap<String, String>,
    pub(crate) nav: Option<Nav>,
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
    /// `value` vem preenchido em inputs (`XmlInputChanged`); é `None` em
    /// cliques (`XmlClick`).
    fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context);
}
