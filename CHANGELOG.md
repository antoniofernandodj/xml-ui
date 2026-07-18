# Changelog

Formato: [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

O crate está em **0.x**: pela convenção do Cargo, um bump de *minor* (`0.40` →
`0.41`) **pode quebrar API**, e é o que este projeto usa para mudanças
incompatíveis. Toda quebra vem listada em **Quebras** com o que fazer para migrar.

---

## [0.51.0] — 2026-07-18

### Adicionado
- **Descompressão gzip transparente no `fetch`.** As requisições agora mandam
  `Accept-Encoding: gzip` por padrão (a menos que o chamador já tenha definido
  um `Accept-Encoding`), e uma resposta com `Content-Encoding: gzip` é
  descomprimida antes de chegar ao Lua — que recebe o mesmo `body` de texto de
  sempre. Um servidor que não comprima ignora o header; o ganho aparece em
  conexões remotas (JSON comprime bem), não em localhost. Só o `fetch`
  unário: o **SSE** (`sse`) continua sem compressão de propósito (stream de
  vida longa exigiria gzip com flush por evento, com taxa pior e atrito com
  proxies). Teste: `gunzip_round_trip`.

## [0.50.1] — 2026-07-18

### Performance
- **`<image>`/`<svg>` agora memoizam o `Handle` por caminho.** Como o
  `render_node` roda a cada redraw, a leitura via `AssetSource` da 0.50.0
  releria o arquivo do disco (dev/`DiskAssets`) ou recopiaria+re-hashearia os
  bytes embutidos (release) a cada quadro, por nó de imagem. Um cache por thread
  (identidade = caminho; assets binários são imutáveis no processo) constrói o
  handle uma vez e o reusa — inclusive entre motores/janelas. Sem mudança de
  API.

## [0.50.0] — 2026-07-18

### Adicionado
- **Camada de resolução de assets (`AssetSource`) — binários standalone.** Todo
  asset que o motor lê em runtime (templates `.gv`/`.kdl`, estilos `.gss`, JSON de
  tema/dados, scripts Luau e binários SVG/imagem) passa por um
  [`AssetSource`](crate::AssetSource) em vez de tocar o `std::fs` direto. O default
  [`DiskAssets`] lê do disco exatamente como antes (com hot-reload), então nada
  muda para quem não fizer nada.
  - Injete uma fonte **embutida** com `GlacierDaemon::assets(Arc<dyn AssetSource>)`
    (ou `GlacierUI::with_asset_source`, `LuauComponent::from_file_with`/
    `from_source_with`) para um binário **100% desacoplado dos arquivos**: um app
    de release pode empacotar todos os seus assets em tempo de compilação (ex.:
    `include_dir!`) atrás de um `AssetSource` e rodar sem a árvore de assets no
    disco. O padrão recomendado é injetar só em release
    (`#[cfg(not(debug_assertions))]`), deixando o dev com disco + hot-reload.
  - Numa fonte embutida, `AssetSource::modified` devolve `None`, o que **desliga o
    hot-reload naturalmente** (`check_reload` vira no-op — não há arquivo a vigiar).
  - Os `<svg>`/`<image>` agora carregam via `from_memory`/`from_bytes` (bytes da
    fonte de assets) em vez de `Handle::from_path`, para funcionarem embutidos.

### Quebras
- `render_node` ganhou um parâmetro final `assets: &dyn AssetSource`. Quem chama o
  motor pela API pública (`GlacierDaemon`/`GlacierApp`/`GlacierUI::render_current`)
  não é afetado; só quem chamava `render_node` diretamente precisa passar a fonte
  (use `&DiskAssets` para o comportamento anterior).

## [0.49.1] — 2026-07-18

### Corrigido
- **`remember_window_geometry` não persistia nada sem um gancho `on_close`.** O
  fechamento da principal só **consultava** a geometria (e, portanto, só disparava
  a gravação) quando havia um `on_close` registrado — a persistência nativa
  ligada por `remember_window_geometry` era ignorada, o `window-geometry.json`
  nunca era escrito e o app reabria sempre no tamanho default. Agora o fechamento
  consulta a geometria quando há `on_close` **ou** a persistência nativa está
  ligada. Regressão: `remember_geometry_consulta_geometria_ao_fechar_sem_on_close`.

## [0.49.0] — 2026-07-18

### Adicionado
- **I/O de arquivo local na camada Luau.** Antes, um `<script>` não tinha como
  ler nem gravar arquivo — a única persistência era o `storage` (JSON chaveado
  gerenciado pelo motor). Agora:
  - **Leitura** via `fetch("file://<caminho>")`: em vez de uma requisição HTTP,
    lê o arquivo local e devolve o **mesmo** formato de sempre
    (`{ ok, status, body, error }` — `200`/`ok` com o conteúdo no `body`,
    `404`/`error` quando não existe). Um script lê um arquivo com a mesma chamada
    com que faria um GET. (Ao contrário do browser, que bloqueia `file://` de
    propósito por ser código remoto; aqui o Luau é código do próprio app.)
  - **Escrita** via o global `write_file(path, conteúdo)` → `true` no sucesso ou
    `false, "<mensagem>"` na falha (cria o diretório pai se preciso; nunca derruba
    o script). Síncrono, como o `storage`.
- **Persistência automática da geometria da janela principal**, opt-in via
  `GlacierDaemon::remember_window_geometry(true)`. Com ela, o tamanho (e a
  posição, onde a plataforma a expõe) é gravado ao fechar e restaurado ao abrir,
  reabrindo o app onde parou — **sem flash** (a janela já nasce no tamanho certo)
  e sempre respeitando o `min_size`. O arquivo (`window-geometry.json`) mora sob o
  `storage_dir`; sem um `storage_dir` a opção é no-op. No Wayland só o tamanho
  volta (o protocolo não expõe a posição ao cliente). Substitui o padrão de um app
  fazer isso à mão via `on_close` + `window::Settings` montadas na inicialização.

## [0.48.0] — 2026-07-17

### Mudado
- **Bandeja: fechar a janela principal agora a recolhe SEM matar o motor.**
  Antes, fechar a última janela (com bandeja) encerrava a janela e **descartava o
  motor** dela — junto com o login e qualquer stream `sse`/`websocket` vivo. Um
  app de bandeja que precisa continuar recebendo eventos (ex.: notificar quando um
  deploy termina) ficava sem conexão nenhuma enquanto recolhido.

  Agora, com bandeja configurada, fechar a principal **destaca** o motor: a janela
  do SO é destruída (no Wayland esconder/minimizar-restaurar é impossível pelo
  toolkit — destruir é a única forma de a janela sumir de verdade), mas o **motor
  segue vivo e headless** — SSE conectado, login intacto, notificações do SO
  continuam disparando. O `open_main()` (item "abrir" da bandeja) **religa esse
  mesmo motor** numa janela nova, preservando a sessão; o `main_id` migra para a
  janela nova (o recipe do `sse`/`websocket` inclui o id da janela, então há um
  breve reconnect do stream no instante da reabertura — irrelevante).

  Sem bandeja, nada muda: fechar a última janela encerra o app como sempre.

### Compatibilidade
- Sem quebras de API. É uma mudança de **comportamento** restrita a apps que usam
  `.tray(...)`: a principal passa a recolher (motor vivo) em vez de destruir. Apps
  sem bandeja não são afetados.

---

## [0.47.0] — 2026-07-17

### Adicionado
- **Ícone de bandeja (system tray) + app que sobrevive à última janela**, atrás
  da feature **`tray`** (opcional; sem ela nada de GTK/tray-icon é arrastado). No
  builder do `GlacierDaemon`:
  - `.tray(TrayConfig { icon, tooltip, items })` — habilita a bandeja. Com ela
    configurada, **fechar a última janela não encerra mais o app**: ele recolhe
    para a bandeja. Sem bandeja, o comportamento é o de sempre (encerra na última
    janela).
  - `.on_tray(|id, &mut TrayActions| { … })` — gancho de clique nos itens do
    menu. `TrayActions` oferece `open_main()` (reabre/foca a principal), `quit()`
    (encerra), `set_label(id, text)` e `set_checked(id, bool)`.
  - `TrayItem::button/check/separator` para montar o menu.
  - Funções globais `notifications_enabled()` / `set_notifications_enabled(bool)`:
    interruptor de processo que o `notify()` consulta antes de emitir — o gancho
    da bandeja liga/desliga as notificações do SO sem passar pela camada Luau.

  A bandeja roda numa **thread dedicada** (o `iced`/`winit` é dono do loop
  principal): Linux via libappindicator+GTK, Windows via message-loop Win32.
  **macOS não é suportado** (exige a thread principal) — lá `.tray(...)` é
  ignorada e o app volta a encerrar na última janela. No Linux não há evento de
  clique no ícone (o clique abre o menu); no Windows o clique esquerdo reabre a
  principal. Ver `src/tray.rs` e `examples/bandeja`.

### Compatibilidade
- Sem quebras: a feature `tray` é opt-in e toda a API nova é aditiva. Quem não a
  habilita compila exatamente como antes (nenhuma dep nova).

---

## [0.46.0] — 2026-07-15

### Mudado
- **`confirm()` (Luau) agora é SÍNCRONO e retorna um booleano**, no espírito do
  `fetch`: suspende a corrotina, exibe o diálogo e só retoma quando o usuário
  escolhe um botão — devolvendo `true` (confirmou) ou `false`
  (cancelou/dispensou). Deixa o fluxo linear, sem callback separado:
  ```lua
  if confirm({ title = "Remover?", message = "…", confirm_label = "Remover",
               destructive = true }) then
      -- fazer a ação aqui mesmo
  end
  ```

### Quebras
- **`confirm{ confirm_action = "…" }` deixou de existir.** Antes, `confirm` não
  suspendia e o botão de confirmação despachava a função nomeada em
  `confirm_action` como um clique à parte. Agora não há `confirm_action`: trate
  o retorno booleano. Migração:
  ```lua
  -- antes
  confirm({ title = "T", message = "M", confirm_action = "do_x" })
  function do_x() ... end
  -- depois
  if confirm({ title = "T", message = "M" }) then ... end
  ```
  Diálogos abertos pela API Rust (`Context::show_dialog` com botões que roteiam
  ações) não mudam — só o `confirm()` da camada Luau passou a suspender.

---

## [0.45.0] — 2026-07-15

### Adicionado
- **`GlacierDaemon::storage_dir(dir)`**: define o diretório onde o global
  `storage` (persistência local em JSON, análoga a `localStorage`) grava seus
  arquivos, aplicado a todas as janelas do app. Sem isto, `storage` mantém o
  comportamento legado — grava em `.glacier-storage/` **relativo ao diretório do
  script**, o que falha silenciosamente quando os assets moram num caminho
  read-only (um app empacotado rodando de `/usr/share`). Passe um diretório
  gravável pelo usuário (ex.: o data dir do XDG) e o `storage` passa a gravar
  em `<dir>/.glacier-storage/<componente>.json`. Também exposto o helper de
  baixo nível `luau::set_storage_root(path)` que o builder usa por baixo.

---

## [0.44.0] — 2026-07-15

### Mudado
- **`notify()` no Linux passa a emitir via `notify-send`** (subprocesso), com
  fallback automático para o `notify-rust` in-process se o `notify-send` não
  estiver instalado. Em outros SOs (Windows/macOS) nada muda — segue in-process.
  Motivo: alguns ambientes de desktop (observado num GNOME 46) **suprimem
  silenciosamente** notificações fdo enviadas *in-process* por um app que tem
  janela — o compositor associa a notificação ao app pelo PID→janela (`app_id`)
  e a descarta mesmo com o app habilitado nas configurações (`.show()` retorna
  `Ok`, nada aparece). Um subprocesso **sem janela** não é associado a nenhum app
  e é exibido. `app_name`/`icon`/título/corpo são repassados aos flags do
  `notify-send`. Ver `emit_os_notification` em `lib.rs`.

---

## [0.43.0] — 2026-07-15

### Adicionado
- **`notify()` ganhou `app_name` e `icon`** (ambos opcionais, na tabela Luau e em
  `NotificationSpec`). `app_name` sobrescreve o padrão do `notify-rust` (que usa o
  nome do executável); `icon` é um nome de ícone do tema ou caminho. Motivação
  real: alguns ambientes de desktop **filtram/descartam** notificações pela
  identidade do app — um GNOME em que o `app_name` casando com um `.desktop`
  instalado fazia a notificação ser descartada silenciosamente (o nome do binário
  virava o `app_name` por padrão). Poder setar um nome de exibição que não casa
  com um `.desktop` contorna isso. `NotificationSpec` agora deriva `Default`.

---

## [0.42.0] — 2026-07-15

### Adicionado
- **`notify(...)` (camada Luau) / `Context::notify` + `NotificationSpec` (Rust)**
  — notificações **nativas do sistema operacional**, entregues à central de
  notificações do SO (freedesktop/D-Bus no Linux/BSD, WinRT no Windows,
  `NSUserNotification` no macOS) via `notify-rust`. Diferente de `toast`, que é
  efêmero e desenhado dentro da própria janela, a notificação sobrevive ao app
  estar minimizado ou em outro workspace — para eventos que o usuário quer saber
  sem olhar para o app (ex.: um deploy terminou). Na Luau: `notify({ title, body })`
  ou `notify("mensagem")` (string vira o corpo). O motor a entrega fora da thread
  de UI (o backend é síncrono), é acumulativa como o toast e não realimenta nada
  ao componente. Novo exemplo: `cargo run --example notificacoes`.

---

## [0.41.0] — 2026-07-14

Rodada de robustez: o que faltava para a lib ser defensável fora do app que a
criou. Ver `RELATORIO_0.38_A_0.40.md` para o processo (inclusive os erros).

### Adicionado
- **`render_inputs::RenderInputs`** — as entradas de render (folhas de estilo,
  templates parseados, viewport) atrás de um portão que conta as mudanças numa
  `epoch`. O cache de avaliação guarda a época em que foi construído e se
  descarta sozinho quando ela avança.
- **CI** (GitHub Actions): build, testes, `clippy -D warnings`, `fmt --check` e
  `cargo doc -D warnings`.
- Este `CHANGELOG.md`.

### Corrigido
- **Invalidação do cache deixou de depender de memória humana.** A 0.40 usava
  oito chamadas manuais de `invalidate_eval_cache()` espalhadas pelos call-sites
  — e uma delas estava furada: o hot-reload de `.gss` escrevia direto em
  `stylesheets[idx]` e só não servia estilo velho porque um `invalidate` genérico
  vinha depois, por acaso. Agora os campos são privados noutro módulo e a época é
  incrementada pelos próprios métodos de mutação.
- **`cargo doc`**: 5 links quebrados na documentação, incluindo um
  `EngineMessage::LuaStream` que não existe (é `LuauStream`).
- **Clippy: 62 → 0**, com `-D warnings`.
- Um resize que **não** cruza breakpoint de `@media` deixou de poder invalidar o
  cache (arrastar a borda da janela custaria uma reconstrução por pixel).

### Quebras
- **`LuauComponent::from_file`/`from_source`** passam a devolver
  `Result<Self, GlacierError>` em vez de `Result<Self, String>`.
  *Migração:* o `Display` do erro traz a mesma mensagem de antes; se você fazia
  `.map_err(|s| ...)` com a `String`, use `.to_string()`.
- Todo o código foi passado por **rustfmt** (commit isolado, sem mudança de
  comportamento) — relevante só para quem mantém um fork.

---

## [0.40.1] — 2026-07-14

### Corrigido
- **Dirty-tracking não funcionava em nenhuma tela com lista.** As variáveis de um
  item de `for-each` (`{l.nome}`) só existem na camada daquele item, mas subiam
  até o conjunto de dependências do *template*. Lá em cima o motor perguntava "o
  contexto ainda tem `l.nome` = a?" e ouvia **não** para sempre, porque `l.nome`
  nunca esteve no contexto — então a tela ficava eternamente suja e o cache
  existia sem nunca acertar. Cada leitura agora registra a profundidade da camada
  que a resolveu, e ao fechar um quadro só sobem as leituras resolvidas fora dele.

## [0.40.0] — 2026-07-14

### Adicionado
- **Dirty-tracking**: o motor rastreia as chaves de contexto que cada subárvore
  lê e **não reconstrói o que não mudou**. Memoiza nas duas fronteiras que pagam:
  o uso de um componente (props bem definidas) e cada item de `for-each`.
  `reevaluate_all` nem entra na árvore se nada que a tela lê mudou.
- `eval::EvalCache`, `eval::evaluate_template`, `eval::Deps`.

Medido na árvore real de um app (600 nós):

| cenário | antes | depois |
|---|---|---|
| muda uma chave que ninguém lê | 6,3 ms | 3,5 µs |
| muda uma chave lida, lista de 45 linhas intacta | 6,3 ms | 1,6 ms |

### Quebras
- **`UiNode` ganhou o campo `node_id`** (identidade estável, é a chave do cache).
  *Migração:* quem constrói `UiNode` à mão precisa preenchê-lo.

### Notas
- Listas **reordenáveis** ficam fora do cache de propósito: o corpo do item
  carrega `drag_order` *injetado* (não lido do contexto), então o rastreamento
  não perceberia uma mudança de ordem. São listas pequenas.

## [0.39.0] — 2026-07-14

### Melhorado
- **`EvalCtx`: contexto em camadas.** Cada item de `for-each` fazia
  `context.clone()` — uma cópia do contexto inteiro (com strings grandes dentro,
  como um log vindo de SSE) por linha renderizada, a cada reavaliação. Agora as
  variáveis do item e as props de componente entram numa cadeia de camadas
  encadeada na pilha, sem copiar a base.

  *Nota honesta:* isto sozinho rendeu pouco (6,5 ms → 6,0 ms). O gargalo era
  outro — ver 0.40.0.

## [0.38.1] — 2026-07-14

### Corrigido
- **Pânico ao parsear qualquer template com caractere multi-byte logo após um
  `<`.** A varredura por `<style>` fatiava o `&str` por byte (`&tail[..5]`); uma
  régua `──` num comentário XML caía no meio de um caractere. Comparação passou a
  ser feita em bytes.

## [0.38.0] — 2026-07-14

### Adicionado
- **Erro tipado (`error::GlacierError`)** com **`Diagnostic`** posicional:
  arquivo, linha, coluna, o trecho ofensor com um `^` embaixo e uma dica
  acionável. Sem dependência nova (`Display`/`Error` à mão).
- **`GlacierDaemon`** ganhou o que faltava para um app real não precisar
  reimplementar o runtime: `.font()`, `.default_font()`, `.main_window(Settings)`,
  `.child_window()`, `.on_message()` (persistência), `.on_close(WindowGeometry)`,
  `.reload_period()`, `.toast_period()`.
- **GSS: lista de seletores por vírgula** (`.a, .b { }`).
- `GlacierUI::keep_evaluated`, `evaluated`, `context`, `current_screen`,
  `history`, `dialog`, `custom_theme`, `stylesheets`, `parsed`, `is_registered`.

### Corrigido
- **Corpo de `<style>` era lido como XML.** Uma tag citada num comentário do CSS
  (`/* o card vira <Text> */`) virava um elemento de verdade, e o erro apontava o
  `</style>` reclamando de uma tag que o autor nunca abriu. Agora o corpo é
  blindado com CDATA e nunca passa pelo parser de XML.
- **`strip_script` comia as linhas do bloco**, deslocando para cima *todo* erro
  abaixo dele. Agora preserva a contagem de linhas.
- **`.a, .b { }` no GSS virava UMA classe de nome literal `"a, .b"`** — sem erro,
  sem aviso, sem estilo, e nenhum nó jamais a casava.
- Comentário de bloco multi-linha no `.gss` deixou de deslocar as linhas
  seguintes.
- Propriedade GSS desconhecida agora avisa com `arquivo:linha` e sugere a certa
  (`colr` → *"você quis dizer 'color'?"*).
- **`window:drag` era um no-op silencioso no Wayland.** O motor resolvia as ações
  `window:*` via `window::latest()`, cujo round-trip perde o pointer-grab serial.
  Passaram a ser tratadas no runner, contra o `Id` da janela em roteamento.

### Melhorado
- **Avaliação escopada.** `reevaluate_all` avaliava **todo template registrado**,
  cada um como raiz — e como avaliar inlina recursivamente os componentes, um app
  com 15 componentes reconstruía a árvore inteira 16 vezes por tecla digitada, 15
  delas para árvores que ninguém renderiza. Agora só a tela ativa (e os
  `keep_evaluated`) é construída.

### Quebras
- **Os campos de `GlacierUI` são privados.** *Migração:* use os getters
  (`context()`, `evaluated(name)`, `current_screen()`, …).
- **`render(name)`** de um template fora de uso devolve `GlacierError::NotEvaluated`
  em vez de uma árvore obsoleta. *Migração:* `set_initial_screen(name)` ou
  `keep_evaluated(name)`.
- **`NodeType::Style` ganhou o campo `line`** (posiciona erros de `.gss` inline).
- Toda a API pública passou de `Result<_, String>` para `Result<_, GlacierError>`.
  *Migração:* o `Display` é compatível; `format!("{e}")` segue funcionando.

---

[0.41.0]: https://github.com/antoniofernandodj/xml-ui/releases/tag/v0.41.0
[0.40.1]: https://github.com/antoniofernandodj/xml-ui/releases/tag/v0.40.1
[0.40.0]: https://github.com/antoniofernandodj/xml-ui/releases/tag/v0.40.0
[0.39.0]: https://github.com/antoniofernandodj/xml-ui/releases/tag/v0.39.0
[0.38.1]: https://github.com/antoniofernandodj/xml-ui/releases/tag/v0.38.1
[0.38.0]: https://github.com/antoniofernandodj/xml-ui/releases/tag/v0.38.0
