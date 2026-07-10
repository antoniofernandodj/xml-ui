//! Aplicação **iced puro** (sem o motor glacier-ui) demonstrando múltiplas
//! janelas: ao clicar no botão, uma nova janela é aberta por cima da janela
//! inicial.
//!
//! Multi-janela no iced 0.14 exige o modelo `daemon` (não `application`),
//! porque só o `daemon` tem um `view` indexado por `window::Id`. O daemon não
//! abre nenhuma janela sozinho: a janela inicial é aberta no `boot`.
//!
//! Rode com:  `cargo run --example janelas_iced`

use iced::widget::{button, center, column, text};
use iced::window;
use iced::{Element, Point, Size, Task};

fn main() -> iced::Result {
    App::bootstrap()
        .run()
}

#[derive(Debug, Clone)]
enum Message {
    /// Pedido de abrir uma nova janela (clique no botão).
    AbrirJanela,
    /// Uma janela terminou de abrir (retornado por `window::open`).
    JanelaAberta(window::Id),
    /// Uma janela foi fechada pelo usuário (via subscription).
    JanelaFechada(window::Id),
}

#[derive(Default)]
struct App {
    /// Janela principal (a primeira, aberta no new).
    principal: Option<window::Id>,
    /// Janelas secundárias abertas por cima.
    filhas: Vec<window::Id>,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        // O daemon não abre janela por conta própria: abrimos a inicial aqui.
        let (id, abrir) = window::open(window::Settings::default());
        (
            App {
                principal: Some(id),
                filhas: Vec::new(),
            },
            abrir.map(Message::JanelaAberta),
        )
    }

    fn title(&self, janela: window::Id) -> String {
        if Some(janela) == self.principal {
            String::from("Janela principal")
        } else {
            String::from("Nova janela")
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AbrirJanela => {
                // Abre uma janela menor, sempre-no-topo e levemente deslocada,
                // de modo a aparecer "por cima" da janela inicial.
                let settings = window::Settings {
                    size: Size::new(360.0, 240.0),
                    position: window::Position::SpecificWith(|tamanho, tela| {
                        Point::new(
                            (tela.width - tamanho.width) / 2.0,
                            (tela.height - tamanho.height) / 2.0,
                        )
                    }),
                    level: window::Level::AlwaysOnTop,
                    ..window::Settings::default()
                };
                let (_id, abrir) = window::open(settings);
                abrir.map(Message::JanelaAberta)
            }
            Message::JanelaAberta(id) => {
                if Some(id) != self.principal {
                    self.filhas.push(id);
                }
                Task::none()
            }
            Message::JanelaFechada(id) => {
                self.filhas.retain(|&f| f != id);
                if Some(id) == self.principal {
                    self.principal = None;
                }
                // Se não sobrou nenhuma janela, encerra o daemon.
                if self.principal.is_none() && self.filhas.is_empty() {
                    iced::exit()
                } else {
                    Task::none()
                }
            }
        }
    }

    fn view(&self, janela: window::Id) -> Element<'_, Message> {

        let abertas = self.filhas.len();
        let a = center(
            column![
                text("Janela principal").size(28),
                text(format!("Janelas abertas por cima: {abertas}")),
                button("Abrir nova janela").on_press(Message::AbrirJanela),
            ]
            .spacing(16)
            .align_x(iced::Alignment::Center),
        );

        let b = center(
            column![
                text("Nova janela").size(28),
                text("Aberta por cima da janela inicial."),
                button("Abrir outra").on_press(Message::AbrirJanela),
            ]
            .spacing(16)
            .align_x(iced::Alignment::Center),
        );

        if Some(janela) == self.principal {
            a.into()
        } else {
            b.into()
        }

    }

    fn subscription(&self) -> iced::Subscription<Message> {
        // Detecta quando qualquer janela é fechada para atualizar o estado.
        let subscription = window::close_events();
        subscription.map(Message::JanelaFechada)
    }

    fn bootstrap() -> iced::Daemon<
        impl iced::Program<
            State = App,
            Message = Message,
            Theme = iced::Theme
        >
    > {
        iced::daemon(App::new, App::update, App::view)
            .title(App::title)
            .subscription(App::subscription)
    }
}
