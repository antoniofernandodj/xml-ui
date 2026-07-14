//! Notificações "toast": info, sucesso, aviso e erro, empilhadas no canto
//! inferior direito da tela e dispensadas sozinhas depois de um tempo.
//!
//! Ao contrário de um [`crate::dialogs::DialogSpec`] — modal, único, bloqueia
//! interação com o resto da tela — um toast é não-modal e cumulativo: vários
//! podem estar em exibição ao mesmo tempo, cada um com seu próprio
//! cronômetro de expiração, e o resto da UI continua clicável por baixo dele.
//!
//! ```ignore
//! ctx.show_toast(ToastSpec::success("Deploy concluído"));
//! ctx.show_toast(ToastSpec::error("Falha ao conectar").with_title("Erro de rede"));
//! ```
//!
//! O motor mantém a lista de toasts ativos (com o instante em que cada um
//! apareceu) fora do [`crate::component::Context`] — assim como o diálogo,
//! isso é aplicado depois que `update()` retorna. A expiração automática
//! depende de um tique periódico (ver [`crate::GlacierUI::toast_subscription`]);
//! sem ele os toasts continuam sendo mostrados/fechados manualmente (clique no
//! "×"), só não somem sozinhos.

use iced::widget::{Space, button, column, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Vector};
use std::time::Duration;

use crate::widget::EngineMessage;

/// O tipo do toast, escolhendo o ícone e a cor de destaque — o mesmo papel de
/// [`crate::dialogs::DialogIcon`], mas sem a variante `Question` (não faz
/// sentido perguntar algo numa notificação que desaparece sozinha) e com
/// `Success` (que os diálogos não têm).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastKind {
    fn glyph(self) -> &'static str {
        match self {
            ToastKind::Info => "ℹ",
            ToastKind::Success => "✓",
            ToastKind::Warning => "⚠",
            ToastKind::Error => "✕",
        }
    }

    /// Cor de destaque (ícone + barra lateral), lida da paleta estendida do
    /// tema ativo — `Success` usa `palette.success`, que os diálogos não têm
    /// motivo para usar (nenhuma variante de `QMessageBox` é "deu certo").
    fn color(self, palette: &iced::theme::palette::Extended) -> Color {
        match self {
            ToastKind::Info => palette.primary.base.color,
            ToastKind::Success => palette.success.base.color,
            ToastKind::Warning => palette.warning.base.color,
            ToastKind::Error => palette.danger.base.color,
        }
    }
}

/// A especificação de um toast: tipo, título opcional (linha em negrito,
/// acima da mensagem), mensagem e por quanto tempo fica em exibição antes de
/// expirar sozinho (ver [`crate::GlacierUI::toast_subscription`]).
///
/// Construída com um dos atalhos [`ToastSpec::info`], [`ToastSpec::success`],
/// [`ToastSpec::warning`], [`ToastSpec::error`] (todos com `duration` padrão
/// de 4 segundos) e ajustada com os builders [`ToastSpec::with_title`] /
/// [`ToastSpec::with_duration`].
#[derive(Debug, Clone)]
pub struct ToastSpec {
    pub kind: ToastKind,
    pub title: Option<String>,
    pub message: String,
    pub duration: Duration,
}

impl ToastSpec {
    /// Tempo padrão em exibição antes de expirar, para os construtores de
    /// conveniência (`info`/`success`/`warning`/`error`).
    const DEFAULT_DURATION: Duration = Duration::from_secs(4);

    /// Um toast do zero, com o tipo, mensagem e duração padrão explícitos.
    pub fn new(kind: ToastKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            title: None,
            message: message.into(),
            duration: Self::DEFAULT_DURATION,
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(ToastKind::Info, message)
    }

    pub fn success(message: impl Into<String>) -> Self {
        Self::new(ToastKind::Success, message)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(ToastKind::Warning, message)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(ToastKind::Error, message)
    }

    /// Anexa um título em negrito, mostrado acima da mensagem.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Substitui a duração padrão (4s) de exibição antes da expiração automática.
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }
}

/// Renderiza a pilha de toasts ativos como um overlay não-modal, ancorado no
/// canto inferior direito da tela. Ao contrário de [`crate::dialogs::overlay`],
/// não cobre a tela inteira com um fundo capturando clique/hover — só os
/// próprios cartões ocupam espaço de interação; clicar fora deles (em
/// qualquer ponto sem um cartão) atravessa para a camada de baixo normalmente.
///
/// `active` é `(id, spec)` de cada toast em exibição, na ordem em que devem
/// aparecer (mais antigo no topo, mais recente embaixo — a ordem de
/// [`crate::GlacierUI`]'s lista interna). Cada cartão tem um botão "×" que
/// despacha [`EngineMessage::ToastDismiss`] com o `id` correspondente.
pub fn overlay<'a>(
    active: impl IntoIterator<Item = (u64, &'a ToastSpec)>,
    theme: &iced::Theme,
) -> Element<'a, EngineMessage> {
    let palette = theme.extended_palette();

    let mut list = column![].spacing(10).align_x(Alignment::End);
    for (id, spec) in active {
        list = list.push(toast_card(id, spec, palette));
    }

    container(list)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(20)
        .align_x(Alignment::End)
        .align_y(Alignment::End)
        .into()
}

/// Renderiza um único cartão de toast, colorido pelo seu [`ToastKind`].
fn toast_card<'a>(
    id: u64,
    spec: &'a ToastSpec,
    palette: &iced::theme::palette::Extended,
) -> Element<'a, EngineMessage> {
    let accent = spec.kind.color(palette);
    let text_color = palette.background.base.text;

    let mut header = row![text(spec.kind.glyph()).size(16).color(accent)]
        .spacing(8)
        .align_y(Alignment::Center);
    if let Some(title) = &spec.title {
        header = header.push(text(title.as_str()).size(14).color(text_color));
    }
    header = header.push(Space::new().width(Length::Fill));
    header = header.push(dismiss_button(id, text_color));

    let card = column![
        header,
        text(spec.message.as_str()).size(13).color(text_color)
    ]
    .spacing(6);

    let card_bg = palette.background.base.color;
    container(card)
        .width(Length::Fixed(320.0))
        .padding(14)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(card_bg)),
            border: Border {
                radius: iced::border::Radius::new(8.0),
                width: 2.0,
                color: accent,
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                offset: Vector::new(0.0, 2.0),
                blur_radius: 10.0,
            },
            ..Default::default()
        })
        .into()
}

/// O botão "×" que fecha um toast antes da expiração automática.
fn dismiss_button<'a>(id: u64, text_color: Color) -> Element<'a, EngineMessage> {
    button(text("×").size(16).color(text_color))
        .padding([0, 6])
        .style(move |_theme: &iced::Theme, status: button::Status| {
            let bg = match status {
                button::Status::Hovered => Color {
                    a: 0.12,
                    ..text_color
                },
                button::Status::Pressed => Color {
                    a: 0.2,
                    ..text_color
                },
                _ => Color::TRANSPARENT,
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color,
                border: Border {
                    radius: iced::border::Radius::new(4.0),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .on_press(EngineMessage::ToastDismiss(id))
        .into()
}
