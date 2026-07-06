# Diálogos modais (`src/dialogs.rs`)

Módulo estilo `QMessageBox` do Qt: diálogos de informação, aviso, erro,
pergunta e confirmação, sobrepostos à tela ativa. Publicado a partir da
versão `0.4.9`.

## Por que é diferente do resto do glacier-ui

O resto do framework é declarativo — a UI vem de um template XML,
cacheada como árvore (`UiNode`) e reavaliada a cada mudança de contexto. Um
diálogo não segue esse caminho: ele é transiente (aberto e fechado por
código, não faz parte de nenhuma tela) e construído inteiramente em Rust
(`src/dialogs.rs`), sem markup.

## API

### `DialogSpec` — a especificação do diálogo

Campos: `icon` (`DialogIcon::Information|Warning|Error|Question|None`),
`title`, `message`, `detail: Option<String>`, `buttons: Vec<DialogButton>`,
`dismissible: bool` (clicar fora fecha sem despachar ação).

Construtores de conveniência, cada um já com os botões e o `dismissible`
corretos (o mesmo papel dos métodos estáticos de `QMessageBox`):

| Construtor | Ícone | Botões | `dismissible` |
|---|---|---|---|
| `DialogSpec::information(title, msg)` | Information | OK | `true` |
| `DialogSpec::warning(title, msg)` | Warning | OK | `true` |
| `DialogSpec::error(title, msg)` | Error | OK | `false` |
| `DialogSpec::question(title, msg)` | Question | No, Yes | `false` |
| `DialogSpec::confirm(title, msg)` | Question | Cancel, OK | `false` |

Builders: `.with_button(DialogButton)`, `.with_detail(texto)` (bloco de
detalhe destacado, tipo `QMessageBox::setDetailedText`), `.dismissible(bool)`.

### `DialogButton` — um botão

`label`, `action` (string roteada ao `Component::update` do dono da tela
quando clicado — mesma convenção de `on_click="..."` num `<Button>`), `role`
(`ButtonRole::Accept|Neutral|Destructive`, só controla a cor). Atalhos:
`ok`, `yes`, `no`, `cancel`, `save`, `discard`, `retry`, `close`.

### Disparando e fechando

De dentro de um `Component::update`:

```rust
fn update(&mut self, action: &str, _value: Option<&str>, ctx: &mut Context) {
    match action {
        "excluir" => ctx.show_dialog(
            DialogSpec::confirm("Excluir projeto", "Essa ação não pode ser desfeita.")
                .with_detail("3 serviços e 2 deployments associados serão removidos.")
                .with_button(DialogButton::discard("excluir_confirmado")),
        ),
        "excluir_confirmado" => { /* ação já veio do botão do diálogo */ }
        _ => {}
    }
}
```

`Context::show_dialog`/`close_dialog` funcionam como `Context::navigate_to`/
`navigate_back`: só marcam a intenção; o motor aplica depois que `update()`
retorna (`route_to_owner`, em `lib.rs`). Fora de um `Component` (direto no
host app), o equivalente é `GlacierUI::show_dialog`/`close_dialog`.

### Renderização

`GlacierUI::render_current` já sobrepõe o diálogo ativo automaticamente —
nenhuma mudança no código do host app:

```rust
Ok(match &self.dialog {
    Some(spec) => iced::widget::stack![screen, dialogs::overlay(spec, &self.theme())].into(),
    None => screen,
})
```

Clicar num botão do diálogo despacha `EngineMessage::DialogButton(action)`;
o `dispatch()` fecha o diálogo e roteia `action` pro `update()` do
componente dono da tela ativa, exatamente como um `UiClick` comum.

## Exemplo

`examples/dialogs/main.rs` + `examples/dialogs/dialogs.xml` — um painel com um botão por
variante (`Information`/`Warning`/`Error`/`Question`/`Confirm com detalhe`).
Rodar com `cargo run --example dialogs`.

## Bug encontrado e corrigido: hover e clique vazando pro que está atrás do modal

### Sintoma

Com o diálogo aberto, passar o mouse sobre um botão da tela por trás dele
mostrava o cursor de mãozinha (`Pointer`) do botão de baixo, como se o
diálogo nem estivesse ali.

### Causa raiz

O `overlay()` empilha duas camadas com `iced::widget::stack![backdrop,
centered]` — `backdrop` é o fundo escurecido que cobre a tela inteira
(`container(Space::new())` com `background: rgba(0,0,0,0.55)`), `centered`
é o cartão do diálogo.

`iced::widget::Stack` decide qual cursor mostrar chamando
`mouse_interaction()` de cada camada, **de cima pra baixo**, e usa a
primeira que devolver algo diferente de `Interaction::None`
(`iced_widget-0.14.2/src/stack.rs`):

```rust
self.children.iter().rev()
    .map(|child| child.mouse_interaction(...))
    .find(|&interaction| interaction != mouse::Interaction::None)
    .unwrap_or_default()
```

O `backdrop` era só um `mouse_area(container(Space::new()))` sem
`.interaction(...)` explícito. Sem esse método, `MouseArea::mouse_interaction`
delega pro conteúdo (`iced_widget-0.14.2/src/mouse_area.rs`):

```rust
match (self.interaction, content_interaction) {
    (Some(interaction), mouse::Interaction::None) if cursor.is_over(layout.bounds()) => interaction,
    _ => content_interaction,
}
```

`Space`/`Container` não têm opinião sobre cursor — devolvem
`Interaction::None`. Para o `Stack`, `None` **não** significa "cursor
padrão"; significa "não sei, pergunta pra camada de baixo". Resultado: ele
ignorava o backdrop inteiro e ia direto checar a tela por trás do modal,
achava o botão sob o cursor e usava o `Pointer` dele.

### Correção

Dar ao `backdrop` uma opinião própria e explícita, usando `Interaction::Idle`
— uma variante do enum `iced_core::mouse::Interaction` **distinta** de
`None`, que existe justamente para dizer "cursor padrão, mas de propósito"
(em vez de "não sei"):

```rust
let backdrop_area = mouse_area(backdrop)
    .interaction(iced::mouse::Interaction::Idle)
    .on_press(EngineMessage::DialogDismiss);
```

### Bug irmão descoberto no processo: clique (não só hover) vazando

Ao investigar o `mouse_interaction`, o `update()` do `MouseArea`
(`iced_widget-0.14.2/src/mouse_area.rs`) revelou um segundo problema, mais
sério: `shell.capture_event()` só é chamado se houver um handler de
`on_press` registrado. Diálogos não-dismissíveis (`error`, `question`,
`confirm`) não tinham `on_press` no backdrop antes da correção — então um
clique nele **não era capturado**: o evento vazava pro `Stack` continuar
procurando na camada de baixo e **acionava de verdade** o botão da tela por
trás do modal, na mesma posição de tela. Não era só cosmético — dava pra
clicar "através" de um diálogo de erro/confirmação.

A correção do hover já resolveu os dois: `on_press(EngineMessage::DialogDismiss)`
ficou **sempre** anexado ao backdrop, mesmo quando `dismissible: false`. O
`dispatch()` (em `lib.rs`) já decidia, de antes, se `DialogDismiss` realmente
fecha o diálogo com base em `spec.dismissible`:

```rust
EngineMessage::DialogDismiss => {
    if self.dialog.as_ref().is_some_and(|d| d.dismissible) {
        self.dialog = None;
    }
    return iced::Task::none();
}
```

Ou seja: anexar `on_press` sempre só muda se o evento é **capturado** (para
de vazar pra camada de baixo); se ele realmente fecha o diálogo continua
sendo decidido só por `dismissible`, sem mudança de comportamento visível
pro usuário em diálogos não-dismissíveis.

### Lição para o resto do framework

Qualquer widget futuro que precise "bloquear" uma área por cima de outra
camada de um `Stack` (outro tipo de overlay, um tooltip, etc.) precisa dos
dois cuidados juntos: `.interaction(Interaction::Idle)` (não deixar
`mouse_interaction` cair em `None`) **e** um `on_press`/`on_release` sempre
presente (não deixar o `update()` deixar de capturar o evento) — só cobrir
visualmente uma área não basta para bloqueá-la nas duas frentes de input do
`iced`.
