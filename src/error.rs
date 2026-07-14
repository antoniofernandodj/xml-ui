//! Erro tipado do glacier-ui ([`GlacierError`]) e o diagnóstico posicional
//! ([`Diagnostic`]) que ele carrega para erros de sintaxe.
//!
//! O motor lida com **quatro linguagens** (XML dos templates, `.gss` dos
//! estilos, JSON de dados/tema e Luau dos scripts), quase sempre vindas de
//! arquivos que o autor do app escreveu à mão. Um `Result<_, String>` diz *o
//! que* quebrou mas não *onde* — e "onde" é a metade que importa quando o
//! arquivo tem 400 linhas. Por isso todo erro de sintaxe carrega um
//! [`Diagnostic`]: arquivo, linha, coluna, o trecho ofensor com um `^` embaixo
//! e, quando a causa é uma pegadinha conhecida da linguagem, uma `dica`.
//!
//! Formato do `Display` (o que o app imprime):
//!
//! ```text
//! views/home.xml:5:3: esperava a tag de fechamento 'Text', encontrei 'style'
//!   |
//! 5 |   </style>
//!   |   ^
//!   = dica: um `<` dentro do corpo de <style> é lido como XML — envolva o CSS
//!     em /* ... */ não basta; use apenas texto sem tags.
//! ```

use std::fmt;

/// Onde, em qual arquivo e por quê um trecho de fonte (XML/`.gss`) não parseou.
///
/// `line`/`col` são **1-based** e se referem ao arquivo **como o autor o
/// escreveu** — não à forma interna que o motor passa ao parser. Isso é um
/// requisito, não um detalhe: o motor pré-processa o XML (tira o `<script>`,
/// embrulha o documento num nó-raiz sintético) e o `.gss` (tira comentários), e
/// cada uma dessas passadas mexeria nas posições se não fosse escrita para
/// preservá-las. Ver `eval::strip_script` e `stylesheet::strip_comments`.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    /// Caminho do arquivo, quando o erro veio de um (`None` para markup inline,
    /// ex.: um `Template::Inline` ou um `<style>` de teste).
    pub file: Option<String>,
    /// Nome do componente que declarou o trecho, quando conhecido — o que o
    /// autor procura quando o arquivo é `components/card.xml` mas o erro
    /// aparece ao registrar a tela `home`.
    pub component: Option<String>,
    /// Linha (1-based) no arquivo original.
    pub line: u32,
    /// Coluna (1-based) no arquivo original.
    pub col: u32,
    /// A mensagem do parser subjacente, já traduzida para algo acionável.
    pub message: String,
    /// A linha ofensora, verbatim (para o `Display` desenhar o `^`). `None`
    /// quando a fonte não estava disponível ao construir o erro.
    pub snippet: Option<String>,
    /// Dica opcional: o que provavelmente causou isso e como sair. Só é
    /// preenchida quando o motor reconhece a pegadinha (ver
    /// [`Diagnostic::with_hint`]).
    pub hint: Option<String>,
}

impl Diagnostic {
    /// Diagnóstico em `line`:`col` com `message`, sem arquivo nem trecho —
    /// enriquecido depois por [`Diagnostic::in_file`] / [`Diagnostic::with_hint`].
    pub fn new(line: u32, col: u32, message: impl Into<String>) -> Self {
        Self {
            file: None,
            component: None,
            line,
            col,
            message: message.into(),
            snippet: None,
            hint: None,
        }
    }

    /// Associa o diagnóstico a um arquivo e recorta dele a linha ofensora
    /// (`snippet`), que o `Display` usa para desenhar o `^` sob a coluna.
    pub fn in_file(mut self, path: impl Into<String>, source: &str) -> Self {
        self.file = Some(path.into());
        self.snippet = source
            .lines()
            .nth(self.line.saturating_sub(1) as usize)
            .map(|l| l.to_string());
        self
    }

    /// Recorta a linha ofensora de `source` sem associar arquivo — para markup
    /// inline, que não tem caminho mas tem fonte.
    pub fn with_source(mut self, source: &str) -> Self {
        self.snippet = source
            .lines()
            .nth(self.line.saturating_sub(1) as usize)
            .map(|l| l.to_string());
        self
    }

    /// Anota o componente que declarou o trecho.
    pub fn in_component(mut self, name: impl Into<String>) -> Self {
        self.component = Some(name.into());
        self
    }

    /// Define o trecho ofensor explicitamente, para quando a linha reportada
    /// (`self.line`, absoluta no arquivo) **não** indexa a fonte que temos em
    /// mãos. É o caso de um `.gss` inline: o corpo do `<style>` começa na linha
    /// 200 do XML, então a linha 3 *dele* é a linha 202 *do arquivo* — quem
    /// recorta o trecho é o chamador, que conhece os dois sistemas de
    /// coordenadas. Ver [`crate::stylesheet::parse_gss_in`].
    pub fn with_snippet(mut self, text: impl Into<String>) -> Self {
        self.snippet = Some(text.into());
        self
    }

    /// Associa o arquivo sem recortar trecho (o chamador já o forneceu via
    /// [`Diagnostic::with_snippet`]).
    pub fn from_file(mut self, path: impl Into<String>) -> Self {
        self.file = Some(path.into());
        self
    }

    /// Anexa a dica de como sair do erro.
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = self.file.as_deref().unwrap_or("<inline>");
        write!(f, "{}:{}:{}: {}", file, self.line, self.col, self.message)?;
        if let Some(comp) = &self.component {
            write!(f, " (no componente '{comp}')")?;
        }

        // Trecho com o `^` sob a coluna ofensora, no estilo do rustc. A
        // "calha" é dimensionada pelo número da linha para o `|` alinhar.
        if let Some(src) = &self.snippet {
            let num = self.line.to_string();
            let gutter = " ".repeat(num.len());
            write!(f, "\n{gutter} |\n{num} | {src}")?;
            // O caret conta em *caracteres* (não bytes) para não sair do lugar
            // com acento/emoji antes dele, e re-emite os TABs da linha para o
            // alinhamento sobreviver a indentação com tab.
            let col = self.col.saturating_sub(1) as usize;
            let pad: String = src
                .chars()
                .take(col)
                .map(|c| if c == '\t' { '\t' } else { ' ' })
                .collect();
            write!(f, "\n{gutter} | {pad}^")?;
        }

        if let Some(hint) = &self.hint {
            write!(f, "\n  = dica: {hint}")?;
        }
        Ok(())
    }
}

/// O erro do glacier-ui. Toda função pública que pode falhar devolve
/// [`Result<T>`] com este erro — nunca mais uma `String` solta.
///
/// `#[non_exhaustive]`: variantes novas podem aparecer numa versão menor, então
/// um `match` de fora da crate precisa de um braço `_`.
#[derive(Debug)]
#[non_exhaustive]
pub enum GlacierError {
    /// Não deu para ler um arquivo do disco (template, `.gss`, `.luau`, dados,
    /// tema). `what` diz o papel do arquivo para a mensagem ficar acionável
    /// ("não consegui ler o stylesheet 'x.gss'"), já que o caminho sozinho não
    /// diz por que o motor o queria.
    Io {
        what: &'static str,
        path: String,
        source: std::io::Error,
    },
    /// O XML de um template não parseou. Ver [`Diagnostic`].
    Xml(Box<Diagnostic>),
    /// Um `.gss` não parseou. Ver [`Diagnostic`].
    Gss(Box<Diagnostic>),
    /// Um JSON (`<link rel="data">` ou tema) não parseou.
    Json {
        path: String,
        source: serde_json::Error,
    },
    /// O JSON até parseou, mas não descreve um tema válido (falta uma cor,
    /// hex inválido, não é objeto).
    Theme { path: String, message: String },
    /// O script Luau de um componente falhou ao carregar ou ao rodar.
    Luau { component: String, message: String },
    /// Um `<link rel="...">` é desconhecido ou está incompleto.
    Link { component: String, message: String },
    /// [`crate::GlacierUI::render_current`] sem tela ativa — falta chamar
    /// [`crate::GlacierUI::set_initial_screen`].
    NoActiveScreen,
    /// Pediram para renderizar/navegar para um componente que não foi
    /// registrado (ou cujo nome está escrito diferente).
    UnknownComponent(String),
    /// O componente existe e está registrado, mas não está **avaliado** — só a
    /// tela ativa e os templates fixados ficam. Erro próprio (e não um
    /// `UnknownComponent` genérico) porque a causa e a saída são outras: não é
    /// um nome errado, é um template fora de uso.
    NotEvaluated(String),
}

impl fmt::Display for GlacierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { what, path, source } => {
                write!(f, "não consegui ler o {what} '{path}': {source}")
            }
            Self::Xml(d) => write!(f, "erro de XML — {d}"),
            Self::Gss(d) => write!(f, "erro de GSS — {d}"),
            Self::Json { path, source } => write!(f, "'{path}' não é um JSON válido: {source}"),
            Self::Theme { path, message } => write!(f, "tema '{path}' inválido: {message}"),
            Self::Luau { component, message } => {
                write!(f, "script Luau do componente '{component}': {message}")
            }
            Self::Link { component, message } => {
                write!(f, "<link> do componente '{component}': {message}")
            }
            Self::NoActiveScreen => write!(
                f,
                "nenhuma tela ativa: chame set_initial_screen(\"<componente>\") antes de renderizar"
            ),
            Self::UnknownComponent(name) => write!(
                f,
                "componente '{name}' não registrado: confira o nome no <import>/register"
            ),
            Self::NotEvaluated(name) => write!(
                f,
                "componente '{name}' está registrado mas não avaliado — só a tela ativa fica \
                 avaliada. Chame set_initial_screen(\"{name}\") para exibi-lo, ou \
                 keep_evaluated(\"{name}\") para renderizá-lo em paralelo à tela"
            ),
        }
    }
}

impl std::error::Error for GlacierError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl GlacierError {
    /// Atalho para [`GlacierError::Io`], usado por todo call-site de
    /// `fs::read_to_string` no motor.
    pub(crate) fn io(what: &'static str, path: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            what,
            path: path.into(),
            source,
        }
    }

    /// O diagnóstico posicional deste erro, quando ele tem um (XML/GSS). Deixa
    /// um app construir a própria UI de erro (ex.: abrir o editor na linha) em
    /// vez de reparsear a mensagem formatada.
    pub fn diagnostic(&self) -> Option<&Diagnostic> {
        match self {
            Self::Xml(d) | Self::Gss(d) => Some(d),
            _ => None,
        }
    }

    /// Anota o componente dono do trecho, quando o erro é posicional. Chamado
    /// pelas camadas de cima (`register_component`), que sabem o nome que o
    /// parser — que só vê texto — não sabe.
    pub(crate) fn in_component(mut self, name: &str) -> Self {
        if let Self::Xml(d) | Self::Gss(d) = &mut self
            && d.component.is_none()
        {
            d.component = Some(name.to_string());
        }
        self
    }
}

/// `Result` do glacier-ui: `Result<T, GlacierError>`.
pub type Result<T> = std::result::Result<T, GlacierError>;

#[cfg(test)]
mod tests {
    use super::*;

    // O Display de um diagnóstico completo traz arquivo:linha:coluna, a linha
    // ofensora e o caret sob a coluna certa — é o contrato que faz o erro ser
    // acionável sem abrir o editor.
    #[test]
    fn display_desenha_caret_na_coluna() {
        let src = "<Column>\n  <Text content=\"a\" />\n</Colunm>\n";
        let d = Diagnostic::new(3, 3, "esperava 'Column', encontrei 'Colunm'")
            .in_file("views/home.xml", src)
            .with_hint("confira a grafia da tag de fechamento");
        let out = d.to_string();

        assert!(out.starts_with("views/home.xml:3:3: esperava"), "{out}");
        assert!(out.contains("3 | </Colunm>"), "{out}");
        // duas casas de recuo (col 3 → 2 espaços) antes do caret
        assert!(out.contains("\n  |   ^"), "caret desalinhado:\n{out}");
        assert!(out.contains("= dica: confira a grafia"), "{out}");
    }

    // Sem fonte disponível (markup inline) o erro ainda diz posição e mensagem,
    // só não desenha o trecho.
    #[test]
    fn display_sem_snippet_ainda_localiza() {
        let d = Diagnostic::new(1, 5, "tag desconhecida");
        let out = d.to_string();
        assert_eq!(out, "<inline>:1:5: tag desconhecida");
    }

    // O caret conta caracteres, não bytes: um acento antes da coluna não pode
    // empurrar o `^` para a direita.
    #[test]
    fn caret_conta_caracteres_nao_bytes() {
        let src = "<Text content=\"ação\" />";
        let d = Diagnostic::new(1, 5, "x").with_source(src);
        let out = d.to_string();
        let caret_line = out.lines().last().unwrap();
        // "  | " + 4 espaços + "^"
        assert!(caret_line.ends_with("|     ^"), "{caret_line:?}");
    }
}
