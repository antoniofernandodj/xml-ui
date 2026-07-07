# Plano: robustecer a camada Luau (`<script>`)

## Motivação

Hoje a camada Luau (`src/luau/mod.rs`, `src/luau/prelude.luau`) tem poucas
responsabilidades: manipula a tabela `ctx` (espelho string→string do
`Context` do motor, ver `sync_to_luau`/`sync_from_luau` em
`src/luau/mod.rs:215-247`), faz um punhado de chamadas de rede (`fetch`,
`sse`, `websocket`) e dispara toasts/confirms (`toast`/`confirm` em
`prelude.luau`).

A analogia mais útil pra pensar no que falta é **JavaScript no browser**: o
`<script>` de um template está pra `ctx`/`fetch`/`toast`/`confirm` assim como
uma página está pra `window`/`fetch`/`Notification`/`alert()`. Boa parte do
que um browser oferece já tem equivalente decente aqui — `fetch`/`sse`/
`websocket` ≈ `fetch`/`EventSource`/`WebSocket`; `json.encode/decode` ≈
`JSON.stringify/parse`; `require` ≈ ES modules. Mas várias APIs básicas de
"plataforma" que um script de browser dá por certas **não existem** na
camada Luau, e algumas ausências são surpreendentes o bastante pra virar bug
silencioso.

Este documento lista as lacunas encontradas, na ordem em que valeria
atacá-las.

## Lacunas identificadas

### 1. Sem navegação (`location`) — maior lacuna

`Context::navigate_to`/`navigate_back` já existem (`src/component.rs:297-304`)
e são usados por componentes Rust, mas o `prelude.luau` **nunca cede** um
pedido de navegação — não há branch `__glacier_nav` em
`LuauComponent::drive` (`src/luau/mod.rs:332-382`), ao lado dos que já
existem para `__glacier_dialog`/`__glacier_toast`. Um `<script>` Luau
simplesmente não consegue trocar de tela sozinho hoje.

**Proposta**: `navigate(screen)` / `navigate_back()` no prelúdio, cedendo
`{ __glacier_nav = true, screen = ... }` / `{ __glacier_nav_back = true }`;
`drive()` ganha os branches correspondentes chamando
`ctx.navigate_to`/`ctx.navigate_back` (não suspende, como toast/dialog).

### 2. Sem timers (`setTimeout`/`setInterval`)

Não há como agendar "daqui a X ms" — debounce, polling ou retry com backoff
hoje só são possíveis contornando via SSE/WebSocket.

**Proposta**: `after(ms, fn_ou_nome)` cedendo um pedido que o motor agenda
como efeito assíncrono (`tokio::time::sleep` + retomada da corrotina/chamada
da função), no mesmo espírito de `PendingFetch`.

### 3. Erros de script são invisíveis ao usuário final

Uma exceção Luau vai só para `eprintln!` (`src/luau/mod.rs:252`, `:521`,
`:527`) — o equivalente a JS sem DevTools console e sem `window.onerror`. Do
ponto de vista do usuário, um bug no script vira "o botão não faz nada",
sem nenhuma pista.

**Proposta**: hook opcional `on_error(msg)` chamado pelo motor quando
`run`/`resume_inner`/`on_stream_event_inner` falham, e/ou promoção
automática para um toast em modo dev.

### 4. Coerção de `ctx` perde dado sem avisar

`luau_value_to_string` (`src/luau/mod.rs:585-600`) devolve `None` para
tabelas/funções — ou seja, `ctx.foo = {1,2,3}` **desaparece silenciosamente**
do contexto, sem erro nem log. É pior que o `"[object Object]"` do browser:
aqui não sobra nem isso.

**Proposta**: já que `json` existe, fazer `ctx` aceitar tabelas
automaticamente (serializando via `json.encode` por baixo) ou, no mínimo,
logar quando um valor é descartado por não ser serializável.

### 5. Sem persistência entre execuções (`localStorage`)

`ctx` é só memória do processo — nada sobrevive a um restart. Se um app
precisa lembrar algo entre aberturas, hoje isso só é resolvível no lado
Rust.

**Proposta**: `storage.get(key)`/`storage.set(key, value)` no prelúdio,
lendo/gravando um arquivo local (JSON), análogo a `localStorage`.

### 6. Sem eventos dinâmicos do motor (viewport, foco, etc.)

Ligação ação→função é estática (nome resolvido em `run_inner`,
`src/luau/mod.rs:256-302`). O motor já emite coisas como
`EngineMessage::Viewport`/`FocusNext` (`src/lib.rs`), mas nada disso chega
ao Luau — não há equivalente a `window.innerWidth`/`resize` nem a
`addEventListener` dinâmico.

**Proposta**: começar pequeno — expor leitura da viewport atual (`viewport()`
→ `{width, height}`) para scripts que precisem reagir a breakpoint além do
que o GSS já resolve via `@media`.

### 7. Fronteira de capacidades não é testada

O interpretador roda com `Lua::new()` "cru" (`Cargo.toml:18`, sem flags de
sandbox explícitas). A boa notícia é que o próprio Luau (dialeto Roblox) já
não expõe `io`/`os.execute` por padrão — mas essa garantia é **implícita**,
não fixada por teste. Uma troca futura de feature flags do `mlua` poderia
furar essa fronteira sem que ninguém notasse.

**Proposta**: teste explícito (`tests/engine_tests.rs` ou
`src/luau/mod.rs`) provando que um script não consegue abrir arquivo
arbitrário nem rodar processo — fixa o contrato de segurança do jeito que o
browser trata JS.

## Fora do escopo

- **Acesso direto à árvore XML (tipo `document.querySelector`)**: o motor
  hoje é "data down, reavalia tudo" (`reevaluate_all`, ver `ROADMAP.md`),
  mais parecido com SSR/React do que DOM mutável. Misturar mutação
  imperativa de nós nesse modelo tende a gerar bugs de sincronização sem
  ganho real, já que `ctx` cobre o caso de uso de forma mais simples.

## Prioridade sugerida

1. Navegação (§1) e timers (§2) — o que mais limita o que um `<script>`
   consegue fazer sozinho hoje.
2. Erro visível (§3) e coerção sem perda silenciosa (§4) — fontes de bugs
   invisíveis em produção.
3. Persistência (§5), eventos de viewport (§6) e teste de fronteira de
   capacidades (§7) — valor incremental, sem urgência.

## Referências

- `src/luau/mod.rs` — `LuauComponent`, `drive`, `sync_to_luau`/`sync_from_luau`.
- `src/luau/prelude.luau` — `fetch`/`sse`/`websocket`/`toast`/`confirm`.
- `src/component.rs` — `Context` (`navigate_to`, `navigate_back`,
  `show_dialog`, `show_toast`, `perform`).
- `ROADMAP.md` — Fase 2 já cogita "Contexto tipado (opcional)", relacionado
  ao ponto §4 deste plano.
