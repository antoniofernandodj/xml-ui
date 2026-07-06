//! Diálogos modais estilo `QMessageBox`: informação, aviso, erro, confirmação
//! e pergunta, além de uma variante totalmente customizável.
//!
//! Ao contrário do resto do glacier-ui (declarado em XML e cacheado como
//! árvore de template), um diálogo é transiente e disparado por código — um
//! [`Component::update`](crate::Component::update) pede
//! [`Context::show_dialog`](crate::Context::show_dialog) com um
//! [`DialogSpec`], o motor o sobrepõe (overlay) à tela atual em
//! [`crate::GlacierUI::render_current`], e o clique num botão chega de volta
//! ao `update()` do mesmo componente como uma ação comum — a mesma rota de
//! um `<Button on_click="...">`.
//!
//! ```ignore
//! ctx.show_dialog(DialogSpec::question("Excluir?", "Essa ação não pode ser desfeita.")
//!     .with_button(DialogButton::no("cancelar"))
//!     .with_button(DialogButton::yes("confirmar_exclusao")));
//! ```

use iced::widget::{button, column, container, mouse_area, row, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Vector};

use crate::widget::EngineMessage;

/// O ícone mostrado ao lado do título, escolhendo também a cor de destaque —
/// o mesmo papel do `QMessageBox::Icon`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogIcon {
    Information,
    Warning,
    Error,
    Question,
    /// Sem ícone (diálogos totalmente customizados).
    None,
}

impl DialogIcon {
    fn glyph(self) -> &'static str {
        match self {
            DialogIcon::Information => "ℹ",
            DialogIcon::Warning => "⚠",
            DialogIcon::Error => "✕",
            DialogIcon::Question => "?",
            DialogIcon::None => "",
        }
    }

    /// Cor de destaque do ícone/título, lida da paleta estendida do tema ativo.
    fn color(self, palette: &iced::theme::palette::Extended) -> Color {
        match self {
            DialogIcon::Information => palette.primary.base.color,
            DialogIcon::Warning => palette.warning.base.color,
            DialogIcon::Error => palette.danger.base.color,
            DialogIcon::Question => palette.primary.base.color,
            DialogIcon::None => palette.background.base.text,
        }
    }
}

/// O papel de um botão, usado só para escolher seu estilo visual (destaque
/// para a ação principal, tom neutro para cancelar, tom de perigo para ações
/// destrutivas) — não afeta o roteamento, que é sempre por `action`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonRole {
    /// Ação principal/afirmativa (OK, Yes, Save) — estilo em destaque (cor primária).
    Accept,
    /// Ação neutra ou de cancelamento (Cancel, No, Close) — estilo discreto.
    Neutral,
    /// Ação destrutiva (Discard, Abort) — estilo de perigo.
    Destructive,
}

/// Um botão do diálogo: rótulo mostrado, ação despachada ao `update()` do
/// componente dono da tela quando clicado (a mesma convenção de
/// `on_click="..."` num `<Button>`), e papel visual.
#[derive(Debug, Clone)]
pub struct DialogButton {
    pub label: String,
    pub action: String,
    pub role: ButtonRole,
}

impl DialogButton {
    /// Um botão com rótulo, ação e papel explícitos.
    pub fn new(label: impl Into<String>, action: impl Into<String>, role: ButtonRole) -> Self {
        Self { label: label.into(), action: action.into(), role }
    }

    /// Atalhos para os botões padrão de um `QMessageBox`. O rótulo é fixo em
    /// inglês (como os `StandardButton` do Qt); use [`DialogButton::new`]
    /// para rótulos localizados.
    pub fn ok(action: impl Into<String>) -> Self { Self::new("OK", action, ButtonRole::Accept) }
    pub fn yes(action: impl Into<String>) -> Self { Self::new("Yes", action, ButtonRole::Accept) }
    pub fn no(action: impl Into<String>) -> Self { Self::new("No", action, ButtonRole::Neutral) }
    pub fn cancel(action: impl Into<String>) -> Self { Self::new("Cancel", action, ButtonRole::Neutral) }
    pub fn save(action: impl Into<String>) -> Self { Self::new("Save", action, ButtonRole::Accept) }
    pub fn discard(action: impl Into<String>) -> Self { Self::new("Discard", action, ButtonRole::Destructive) }
    pub fn retry(action: impl Into<String>) -> Self { Self::new("Retry", action, ButtonRole::Accept) }
    pub fn close(action: impl Into<String>) -> Self { Self::new("Close", action, ButtonRole::Neutral) }
}

/// A especificação de um diálogo modal: ícone, título, mensagem, um texto de
/// detalhe opcional (colapsável no Qt; aqui sempre visível, num bloco
/// destacado — ver [`DialogSpec::with_detail`]) e os botões disponíveis.
///
/// Construída com um dos construtores de conveniência
/// ([`DialogSpec::information`], [`DialogSpec::warning`],
/// [`DialogSpec::error`], [`DialogSpec::question`], [`DialogSpec::confirm`])
/// ou do zero com [`DialogSpec::new`], e ajustada com os métodos builder
/// (`with_*`/`dismissible`).
#[derive(Debug, Clone)]
pub struct DialogSpec {
    pub icon: DialogIcon,
    pub title: String,
    pub message: String,
    pub detail: Option<String>,
    pub buttons: Vec<DialogButton>,
    /// Se `true`, clicar no fundo escurecido fecha o diálogo sem despachar
    /// nenhuma ação (`EngineMessage::DialogDismiss`). Diálogos de erro e de
    /// pergunta/confirmação nascem com isso desligado — o usuário precisa
    /// escolher um botão explicitamente, como no Qt (`exec()` só retorna com
    /// um `StandardButton`).
    pub dismissible: bool,
}

impl DialogSpec {
    /// Um diálogo em branco, sem botões (adicione com [`DialogSpec::with_button`]).
    pub fn new(icon: DialogIcon, title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            icon,
            title: title.into(),
            message: message.into(),
            detail: None,
            buttons: Vec::new(),
            dismissible: true,
        }
    }

    /// `QMessageBox::information` — um único botão OK.
    pub fn information(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(DialogIcon::Information, title, message).with_button(DialogButton::ok("ok"))
    }

    /// `QMessageBox::warning` — um único botão OK.
    pub fn warning(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(DialogIcon::Warning, title, message).with_button(DialogButton::ok("ok"))
    }

    /// `QMessageBox::critical` — um único botão OK; não dispensável clicando
    /// fora, o usuário precisa reconhecer o erro explicitamente.
    pub fn error(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(DialogIcon::Error, title, message)
            .with_button(DialogButton::ok("ok"))
            .dismissible(false)
    }

    /// `QMessageBox::question` — botões Yes/No.
    pub fn question(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(DialogIcon::Question, title, message)
            .with_button(DialogButton::no("no"))
            .with_button(DialogButton::yes("yes"))
            .dismissible(false)
    }

    /// Confirmação genérica de uma ação — botões Cancel/OK.
    pub fn confirm(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(DialogIcon::Question, title, message)
            .with_button(DialogButton::cancel("cancel"))
            .with_button(DialogButton::ok("ok"))
            .dismissible(false)
    }

    /// Adiciona um botão, na ordem em que aparecem da esquerda pra direita.
    pub fn with_button(mut self, button: DialogButton) -> Self {
        self.buttons.push(button);
        self
    }

    /// Anexa um texto de detalhe (`QMessageBox::setDetailedText`), mostrado
    /// num bloco destacado abaixo da mensagem principal.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Define se clicar fora do cartão do diálogo o fecha sem despachar ação.
    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }
}

/// Renderiza o diálogo como um overlay completo: um fundo semitransparente
/// cobrindo toda a área disponível (clicável para dispensar, se
/// `spec.dismissible`) com o cartão do diálogo centralizado por cima. Chame
/// [`crate::GlacierUI::render_current`] normalmente — ele já empilha isto por
/// cima da tela ativa quando há um diálogo em exibição.
pub fn overlay<'a>(spec: &'a DialogSpec, theme: &iced::Theme) -> Element<'a, EngineMessage> {
    let palette = theme.extended_palette();

    let mut header = row![].spacing(10).align_y(Alignment::Center);
    if spec.icon != DialogIcon::None {
        header = header.push(text(spec.icon.glyph()).size(22).color(spec.icon.color(palette)));
    }
    header = header.push(text(spec.title.as_str()).size(18));

    let mut card = column![header, text(spec.message.as_str()).size(14)].spacing(14);

    if let Some(detail) = &spec.detail {
        let detail_bg = palette.background.weak.color;
        let detail_text = palette.background.weak.text;
        card = card.push(
            container(text(detail.as_str()).size(12).color(detail_text))
                .padding(8)
                .width(Length::Fill)
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(Background::Color(detail_bg)),
                    border: Border {
                        radius: iced::border::Radius::new(4.0),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..Default::default()
                }),
        );
    }

    let mut buttons = row![Space::new().width(Length::Fill)].spacing(8);
    for b in &spec.buttons {
        buttons = buttons.push(dialog_button(b, palette));
    }
    card = card.push(buttons);

    let card_bg = palette.background.base.color;
    let card_border = palette.background.strong.color;
    let card_box = container(card)
        .width(Length::Fixed(380.0))
        .padding(20)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(card_bg)),
            border: Border { radius: iced::border::Radius::new(8.0), width: 1.0, color: card_border },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
                offset: Vector::new(0.0, 4.0),
                blur_radius: 16.0,
            },
            ..Default::default()
        });

    let centered = container(card_box)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    let backdrop = container(Space::new())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.55))),
            ..Default::default()
        });

    // `Idle` (not `None`) is deliberate: `iced::widget::Stack` picks the
    // topmost layer whose `mouse_interaction()` isn't `Interaction::None` —
    // `None` doesn't mean "the idle/arrow cursor", it means "no opinion,
    // check the layer underneath". Reporting `None` here (the default,
    // if `.interaction()` is never called) let hover state leak through the
    // backdrop to whatever button sat at the same screen position on the
    // base screen below, showing its hand cursor right through the modal.
    //
    // `on_press` is always attached, even for non-dismissible dialogs, for
    // the same reason: `MouseArea::update` only calls `shell.capture_event()`
    // when it actually has a press handler — without one, a click on the
    // backdrop wouldn't just fail to close the dialog, it would fall through
    // the stack and land on whatever's underneath (a real click-through, not
    // just a cosmetic hover leak). `dispatch()` already checks `dismissible`
    // before honoring `DialogDismiss`, so attaching it unconditionally here
    // only affects event capture, not whether clicking outside closes it.
    let backdrop_area = mouse_area(backdrop)
        .interaction(iced::mouse::Interaction::Idle)
        .on_press(EngineMessage::DialogDismiss);

    iced::widget::stack![Element::from(backdrop_area), centered].into()
}

/// Renderiza um botão do diálogo, colorido pelo seu [`ButtonRole`].
fn dialog_button<'a>(
    b: &'a DialogButton,
    palette: &iced::theme::palette::Extended,
) -> Element<'a, EngineMessage> {
    let base = match b.role {
        ButtonRole::Accept => palette.primary.base.color,
        ButtonRole::Neutral => palette.background.strong.color,
        ButtonRole::Destructive => palette.danger.base.color,
    };
    let text_color = match b.role {
        ButtonRole::Neutral => palette.background.base.text,
        _ => Color::WHITE,
    };

    button(text(b.label.as_str()).color(text_color))
        .padding([8, 16])
        .style(move |_theme: &iced::Theme, status: button::Status| {
            let bg = match status {
                button::Status::Hovered => Color { a: 0.85, ..base },
                button::Status::Pressed => Color { a: 0.7, ..base },
                _ => base,
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color,
                border: Border {
                    radius: iced::border::Radius::new(6.0),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .on_press(EngineMessage::DialogButton(b.action.clone()))
        .into()
}
