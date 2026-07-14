# Changelog

Formato: [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

O crate está em **0.x**: pela convenção do Cargo, um bump de *minor* (`0.40` →
`0.41`) **pode quebrar API**, e é o que este projeto usa para mudanças
incompatíveis. Toda quebra vem listada em **Quebras** com o que fazer para migrar.

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
