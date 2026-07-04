# Plano: efeitos assíncronos que também pedem UI (toast/diálogo/navegação)

## Motivação

Hoje um efeito assíncrono (`Context::perform`, ver `component.rs`) só tem um
canal de volta para o motor:

```rust
pub enum Effect {
    Perform(Pin<Box<dyn Future<Output = Vec<(String, String)>> + Send>>),
}
```

O `future` só pode devolver pares `(chave, valor)`, que o motor mescla no
contexto via `EngineMessage::ContextPatch` — **sem nunca passar pelo
`Component::update()`** (ver `GlacierUI::dispatch`, o braço
`EngineMessage::ContextPatch(pairs) => { merge; reevaluate_all(); }`). Isso
significa que um `future` não tem como pedir nada além de "escreva estes
dados": não pode mostrar um toast (`ctx.show_toast`), abrir um diálogo
(`ctx.show_dialog`) nem navegar (`ctx.navigate_to`) — todos esses métodos são
do `Context`, e não existe `Context` no momento em que o `future` termina.

Isso já mordeu um app host real: o `remote-ui` do rustploy precisava de um
toast de sucesso/erro para operações como deploy, delete e salvar spec — que
são, quase todas, resultado de um `ctx.perform`. Sem um jeito direto de pedir
o toast dali, a solução encontrada foi **contrabandear o pedido dentro dos
próprios pares de dados**, com chaves reservadas:

```rust
// crates/remote-ui/src/app/net.rs
pub const TOAST_KIND_KEY: &str = "__toast_kind";
pub const TOAST_MSG_KEY: &str = "__toast_message";

fn with_outcome_toast(pairs: Vec<(String, String)>, msg: &str) -> Vec<(String, String)> {
    let kind = if msg.starts_with("erro") { "error" } else { "success" };
    // ...push (TOAST_KIND_KEY, kind) e (TOAST_MSG_KEY, msg) em `pairs`
}
```

E o app host precisa interceptar **toda** `ContextPatch` antes dela chegar no
motor, para não deixar essas duas chaves virarem lixo dentro do contexto:

```rust
// crates/remote-ui/src/app/mod.rs
EngineMessage::ContextPatch(pairs) => {
    let (rest, toast) = extract_toast(pairs); // separa as chaves reservadas
    if let Some(spec) = toast {
        self.motor.show_toast(spec);
    }
    EngineMessage::ContextPatch(rest)
}
```

Funciona, mas é claramente um contorno: duas convenções (chaves
`"__toast_*"` e um passo de extração) que só existem por causa de uma
limitação de tipo do `Effect::Perform`, e que qualquer novo app host
precisaria reinventar sozinho — o motor não documenta nem oferece isso, é
100% lógica do app.

## Objetivo

Deixar um efeito assíncrono pedir toast (e, no limite, diálogo/navegação) do
mesmo jeito natural que o código síncrono de `update()` já pede — sem chaves
reservadas, sem interceptação no app host.

## Desenho proposto

### 1. Um tipo de retorno mais rico para o efeito

```rust
// component.rs
/// O que um efeito assíncrono pode pedir ao terminar, além de dados —
/// mesmo vocabulário de pedidos que `Context` já expõe para código síncrono
/// (`show_toast`, `show_dialog`, `navigate_to`), só que aplicado depois que
/// o `future` resolve, não depois que `update()` retorna.
#[derive(Default)]
pub struct EffectOutcome {
    pub patch: Vec<(String, String)>,
    pub toast: Option<crate::toasts::ToastSpec>,
    // dialog/nav ficam de fora da Fase 1 (ver "Não está no escopo") — dá pra
    // acrescentar depois com o mesmo mecanismo, sem quebrar a Fase 1.
}

// Construtor de conveniência para o caso comum (só dados, sem toast) — mantém
// `perform` ergonômico para o uso majoritário (fetch/refresh).
impl From<Vec<(String, String)>> for EffectOutcome {
    fn from(patch: Vec<(String, String)>) -> Self {
        Self { patch, toast: None }
    }
}
```

### 2. `Context::perform` aceita o tipo mais rico sem quebrar quem só devolve dados

```rust
impl<'a> Context<'a> {
    pub fn perform<F, T>(&mut self, future: F)
    where
        F: Future<Output = T> + Send + 'static,
        T: Into<EffectOutcome>,
    {
        self.effects.push(Effect::Perform(Box::pin(async move {
            future.await.into()
        })));
    }
}
```

`T: Into<EffectOutcome>` é o que preserva compatibilidade: todo `ctx.perform`
existente que devolve `Vec<(String, String)>` continua compilando sem
mudar uma linha (usa o `From` acima). Quem quiser pedir um toast passa a
devolver `EffectOutcome` diretamente:

```rust
ctx.perform(async move {
    let msg = run_command(...).await;
    EffectOutcome { patch: vec![...], toast: Some(ToastSpec::success(msg)) }
});
```

### 3. `dispatch()` aplica o toast no mesmo lugar que já aplica `ContextPatch`

```rust
// lib.rs — route_to_owner, ao converter `Effect::Perform` em `iced::Task`
Effect::Perform(future) => iced::Task::perform(future, |outcome: EffectOutcome| {
    EngineMessage::EffectOutcome(outcome)
}),
```

Novo braço em `dispatch()`, irmão do de `ContextPatch`:

```rust
EngineMessage::EffectOutcome(outcome) => {
    for (k, v) in outcome.patch {
        self.context_data.insert(k, v);
    }
    if let Some(spec) = outcome.toast {
        self.show_toast(spec);
    }
    let _ = self.reevaluate_all();
    return iced::Task::none();
}
```

`EngineMessage::ContextPatch` continua existindo (é usado por
`Component::subscription`, que devolve `EngineMessage` puro, fora do escopo
de `Effect`) — só o caminho de `Effect::Perform` passa a produzir
`EffectOutcome` por baixo.

### 4. Migração do `remote-ui`

Depois de publicada, o `remote-ui` deixa de precisar de `TOAST_KIND_KEY`/
`TOAST_MSG_KEY`/`extract_toast`: cada função de `net.rs` passa a devolver
`EffectOutcome { patch, toast }` diretamente, e a interceptação em
`app/mod.rs`'s `update()` some por completo — o toast chega pelo caminho
normal do motor.

## Fases

**Fase 1 (mínima, é a proposta acima)** — só toast. Cobre 100% do caso real
já em produção no `remote-ui`.

**Fase 2 (opcional, se aparecer necessidade real)** — estender
`EffectOutcome` com `dialog: Option<DialogAction>` e `nav: Option<Nav>`,
seguindo exatamente o mesmo padrão. Só vale a pena se algum app host precisar
abrir um diálogo ou navegar como reação direta a um resultado assíncrono (o
`remote-ui` hoje não precisa — os diálogos dele são todos de confirmação
*antes* da chamada assíncrona, não depois).

## Não está no escopo

- Mudar `Component::subscription`'s `EngineMessage::ContextPatch` (esse já é
  produzido fora do `Effect`, por streams de longa duração como
  `poll_stream` — pedir toast dali continua sendo uma decisão do app host,
  não do motor, porque a subscription já emite `EngineMessage` bruto, não
  passa pelo `Effect::Perform`/`Context::perform`).
- Diálogo/navegação a partir de um efeito (Fase 2, só se necessário).
- Múltiplos toasts por efeito (`EffectOutcome.toast` fica `Option<ToastSpec>`,
  um só) — nenhum caso real pediu mais de um até agora.

## Referências

- Limitação atual: `component.rs` (`Effect`, `Context::perform`), `lib.rs`
  (`route_to_owner`, braço `EngineMessage::ContextPatch` do `dispatch`)
- Módulo de toasts: `src/toasts.rs`, `TOASTS.md` (se existir) / comentário de
  módulo em `toasts.rs`
- Contorno atual em produção (a remover após a Fase 1):
  `rustploy/crates/remote-ui/src/app/net.rs` (`TOAST_KIND_KEY`,
  `TOAST_MSG_KEY`, `with_toast`, `with_outcome_toast`) e
  `rustploy/crates/remote-ui/src/app/mod.rs` (`extract_toast`)
