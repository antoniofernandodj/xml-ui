//! Trait-based shortcut for wiring an app into `iced::application`, so a host
//! app's `main` doesn't need to name `App::init`/`App::update`/`App::view`
//! by hand (and, combined with the `iced` re-export at the crate root,
//! doesn't need `iced` as a direct dependency just to call `.run()`).
//!
//! **Nota:** este é o atalho para apps de **janela única** (`iced::application`).
//! Para o modelo multi-janela (abrir componentes/XML em novas janelas via
//! [`crate::Context::open_window`] / `open_window(...)` na Lua), use o runner
//! [`crate::GlacierDaemon`], que é o caminho recomendado para novos apps.

use iced::{Element, Subscription, Task};

/// Implement this on an app's root state to get [`GlacierApp::bootstrap`], a
/// drop-in replacement for `iced::application(App::init, App::update, App::view)`
/// that also wires `subscription` in. The builder returned still takes every
/// other `iced::application(...)` setting (`title`, `theme`, `font`, `window`,
/// ...) exactly as before — only the four methods below are pre-wired.
pub trait GlacierApp: Sized + 'static {
    /// The message type produced by [`update`](GlacierApp::update),
    /// [`view`](GlacierApp::view) and [`subscription`](GlacierApp::subscription).
    type Message: Send + 'static;

    /// Builds the initial state and any startup task.
    fn init() -> (Self, Task<Self::Message>);

    /// Reacts to a message, mutating state and optionally issuing a task.
    fn update(&mut self, message: Self::Message) -> Task<Self::Message>;

    /// Renders the current state.
    fn view(&self) -> Element<'_, Self::Message>;

    /// Long-lived event sources (timers, sockets, ...). Defaults to none.
    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }

    /// Starts the `iced::application` builder with
    /// [`init`](GlacierApp::init), [`update`](GlacierApp::update),
    /// [`view`](GlacierApp::view) and [`subscription`](GlacierApp::subscription)
    /// already wired in.
    fn bootstrap() -> iced::Application<
        impl iced::Program<State = Self, Message = Self::Message, Theme = iced::Theme>,
    > {
        iced::application(Self::init, Self::update, Self::view).subscription(Self::subscription)
    }
}
