# Plano: evolução do GSS — resolver limitações conhecidas

Mapa das limitações do motor `.gss` (diagnóstico feito sobre a 0.4.14) e um
backlog **ordenado por custo-benefício** para atacá-las uma a uma. Cada item
traz problema, benefício, custo estimado, esboço de implementação e status.

> Regra do fluxo (ver rustploy `CLAUDE.md`): mudança aqui → bump de versão →
> `cargo publish` → subir a dep em `rustploy-gui`. Para não fazer um release por
> item minúsculo, os itens de mesmo tier podem ser **publicados em lote**.

---

## Ordem de ataque (melhor ratio primeiro)

| # | Item | Benefício | Custo | Tier | Status |
|---|------|-----------|-------|------|--------|
| 1 | Classe duplicada faz **merge** (não clobber) | Médio | Trivial | 1 | ✅ feito |
| 2 | Propriedade desconhecida: **skip + warn** (não derruba o arquivo) | Médio | Trivial | 1 | ✅ feito |
| 3 | `width/height` com **pesos** (`fill N` / FillPortion) | Médio | Baixo | 1 | ✅ feito |
| 4 | **max** width & height (via wrap em container) | Médio-alto | Baixo-médio | 1 | ✅ feito |
| 5 | **Cor do texto do botão** configurável (desembutir branco) | Médio | Baixo | 1 | ✅ feito |
| 6 | **Variáveis / design tokens** (`:root` + `var(--x)`) | **Altíssimo** | Médio | 2 | ✅ feito (0.8.0) |
| 7 | **Pseudo-estados** `:hover` / `:focus` / `:disabled` / `:active` | Alto | Alto | 3 | ✅ feito |
| 8 | **`@media`** (responsivo) | Médio | Alto | 3 | ✅ feito (0.9.0) |
| 9 | Seletores **compostos/descendentes** + especificidade + `!important` | Médio-baixo | Alto | 4 | ⬜ |
| 10 | Propriedades extras sob demanda (opacity, shadow, borda por-lado, gradiente radial, transform) | Pontual | Médio | 4 | ⬜ |
| 11 | Seletor de **id** (`#nome`) + `#nome:estado` | Alto | Baixo | 2 | ✅ feito |
| 12 | Seletor de **tag** (`Button {}`, incl. minúsculo) | Baixo | Baixo-médio | 4 | ⬜ |

Fora de escopo: `margin` (o iced usa `padding`+`spacing`, não há caixa de margem).

> **Nota sobre seletor de tag (#12).** Revisado: **é factível** (a tag já é
> conhecida no parse), mas o valor é baixo e há duas armadilhas — (a) o namespace
> de tag é compartilhado entre **builtins** e **componentes do usuário**
> (`parser.rs:594` transforma tag desconhecida em `Component`), e como componentes
> são **inlinados antes da resolução de estilo**, `Card {}` nunca casaria (só tipos
> builtin); (b) um `Button {}` global fura a **encapsulação** de todo componente.
> Se for feito: só builtins, especificidade mais baixa (tag < classe < id < inline),
> tag normalizada para minúsculo (`Button {}` == `button {}`), documentar o raio
> de alcance. O caso desejado ("estilizar um componente pelo nome") fica com
> **id/classe**, não com tag.

---

## Tier 1 — Quick wins (baixo custo, bom retorno)

### 1. Classe duplicada → merge em vez de clobber  ✅
**Problema.** Dois blocos `.card { }` no mesmo arquivo: o segundo **apaga** o
primeiro inteiro (`rules.insert(name, rule)` em `stylesheet.rs`), em vez de
mesclar como o CSS faz. Footgun silencioso.
**Fix.** Em `parse_gss`, ao inserir uma regra cujo nome já existe, `merge_from`
sobre a existente em vez de substituir.
**Arquivos.** `src/stylesheet.rs` (`parse_gss`). + teste.

### 2. Propriedade desconhecida → skip + warn  ✅
**Problema.** Um `wibble: 1;` (ou um typo) faz `parse_gss` retornar `Err` e
**derruba o arquivo `.gss` inteiro** — todas as regras somem. Áspero demais.
**Fix.** Em `parse_rule_body`, pular a declaração desconhecida com `eprintln!`
de aviso (com seletor + chave), mantendo o resto da regra e do arquivo. Erros
estruturais (sem `:`, valor vazio, número inválido) continuam sendo erro.
**Arquivos.** `src/stylesheet.rs` (`parse_rule_body`). + ajustar teste
`unknown_property_is_an_error`.

### 3. Pesos em `width`/`height` (FillPortion)  ✅
**Problema.** `parse_length` só aceita `fill | shrink | <px>`; sem pesos de flex
(`Length::FillPortion`) nem `%`. Não dá para fazer "coluna A ocupa 2x a B".
**Fix (feito).** `parse_length` aceita `fill N` / `fill-N` → `FillPortion(N)`
(e ficou case-insensitive de brinde; `fill 0` normaliza p/ 1). `%` ficou de fora.
**Arquivos.** `src/widget.rs` (`parse_length`) + `mod length_tests` (4 testes).

### 4. `max-width` / `max-height`  ✅
**Problema.** Sem limites de tamanho; `form_panel` fixa `width: 640` na unha.
**Fix (feito).** Campos `max_width`/`max_height` (f32) em `StyleRule`/`UiNode`
(GSS `max-width`/`max-height`, attr `maxWidth`/`maxHeight`). Como `Row`/`Column`
do iced não capam o próprio tamanho, o `render_node` **envolve qualquer nó com
teto num `container().max_width()/.max_height()`** (ponto único antes do
`mouse_area`) — vale para todo tipo de nó, não só Container. `min-*` ficou de
fora (o iced não expõe fácil; usar por ora).
**Arquivos.** `stylesheet.rs`, `parser.rs`, `eval.rs`,
`widget.rs`. + teste de parse.

### 5. Cor do texto do botão configurável  ✅
**Problema.** O rótulo do botão era `Color::WHITE` fixo (`widget.rs`); o `color`
do botão pinta só o **fundo**. Impossível botão de texto escuro/tema.
**Fix (feito).** Prop **UiNode-level** `text_color` (GSS `text-color`, attr
`textColor`) — mesma camada de `font`/`text_align`, sem tocar em
`NodeType::Button`. Resolvido no `eval` (inline > classe) e aplicado no
`button::Style.text_color`; default branco (não quebra nada).
**Arquivos.** `stylesheet.rs`, `parser.rs`, `eval.rs`,
`widget.rs`.

---

## Tier 2 — Alto valor, custo médio

### 6. Variáveis / design tokens (`:root { --x } ` + `var(--x)`)  ✅ (0.8.0)
**Problema.** Sem `var()`/custom properties: cada hex é repetido em dezenas de
regras (ver `app.gss` do rustploy). A paleta não tem fonte única e o `theme.json`
não é referenciável do `.gss`. Reuso hoje = criar uma classe.
**Fix (feito).**
- `:root { --nome: valor; }` é o único seletor não-classe aceito (`parse_root_vars`);
  a chave guarda o `--nome` completo.
- `var(--nome)` e `var(--nome, fallback)` substituídos nos campos String da regra
  **no `resolve_classes`** (helper `substitute_vars` + `StyleRule::resolve_var_refs`),
  não no parse — assim as variáveis são **cross-sheet** (paleta no `app.gss` global
  resolve `var()` em regras de qualquer sheet com escopo, respeitando a prioridade
  de layering). Var indefinida sem fallback → string vazia. Sem recursão (1 passada).
- Tudo contido no `stylesheet.rs` (não tocou parser/eval/widget). `var()` inline em
  atributo do XML ainda NÃO resolve (fica p/ v2) — só em valores de classe `.gss`.
**Arquivos.** `src/stylesheet.rs` + 4 testes (`root_vars_resolve_via_var`,
`var_fallback_and_undefined`, `vars_are_cross_sheet`, `var_embedded_in_gradient`).
**Impacto no rustploy (a fazer):** adotar `:root` no `app.gss` e trocar os hex
repetidos por `var(--x)` — colapsa a paleta em ~6 tokens. Requer subir a dep p/ 0.8.0.

---

## Tier 3 — Alto valor, custo alto (arquitetural)

### 7. Pseudo-estados `:hover` / `:focus` / `:disabled` / `:active`  ✅
**Problema.** O estilo era resolvido **uma vez** (estático). Hover/pressed de
botão eram auto-derivados (±10% de luminância) da única `color`; não havia
hover/focus/disabled reais e configuráveis para nenhum elemento.
**Fix (feito).**
- `.classe:estado { }` — segunda quebra (com `:root`) da regra "só seletor de
  classe". `PseudoState` (`Hover`/`Focus`/`Active`/`Disabled`, `pressed` como
  alias de `active`) parseado em `parse_gss`; guardado à parte em
  `StyleSheet.states: HashMap<classe, HashMap<PseudoState, StyleRule>>` (nunca
  em `rules`). Classe duplicada com o mesmo estado faz merge, como a regra
  base. Threadado também por dentro de `@media` (`MediaQuery.states`).
- `resolve_state_classes(classes, sheets, viewport) -> StateStyles` — mesmo
  pipeline classes→sheets→`@media`→`var()` de `resolve_classes`, devolvendo os
  4 overlays (vazios quando não declarados).
- `eval.rs` resolve `state_styles` junto da regra base (mesma chamada de
  `process_template` na lista de classes) e embrulha cada overlay não-vazio em
  `UiNode::{hover,focus,active,disabled}_style: Option<Box<StyleRule>>` — só
  aloca quando o `.gss` realmente declara aquele estado.
- **`disabled`** — novo atributo inline (`disabled`/`desabilitado`, só
  atributo, sem `.classe { }` equivalente). Sem handler anexado
  (`on_press`/`on_input`/`on_toggle`), o próprio iced já reporta
  `Status::Disabled` — o motor não rastreia estado nenhum manualmente, reusa
  a máquina de estado nativa de cada widget (por isso o pseudo-estado só se
  aplica a widgets cujo `Status` do iced cobre aquele conceito).
- **Cobertura por widget** (`widget.rs`): `Button` — `:hover`/`:active`/
  `:disabled` completos (requer uma `color` base na classe; sem overlay,
  cai no auto-derive histórico). `TextInput` — `:hover`/`:focus`/`:disabled`
  completos, por cima de `text_input::default`. `Select`/`pick_list` — só
  `:hover` (iced não tem `Status::Disabled` para ele). `Checkbox`/`Toggle` —
  só o atributo `disabled` (visual padrão do tema; overlay de cor por estado
  ainda não implementado — próximo passo natural, mesma infra).
**Arquivos.** `stylesheet.rs` (`PseudoState`, `StateStyles`,
`resolve_state_classes`, parse + 5 testes), `parser.rs` (`UiNode::disabled` +
4 campos `*_style`), `eval.rs` (resolução), `widget.rs` (Button/TextInput/
Select/Checkbox/Toggle). README atualizado com seção própria.
**Exemplo.** `examples/pseudo_estados/` (`main.rs`/`.gv`/`.gss`/`theme.json`) —
Button hover/active/disabled, TextInput hover/focus/disabled, Select hover,
Checkbox/Toggle disabled, lado a lado. **Rodado e conferido visualmente**
(`cargo run --example pseudo_estados`): hover clareia o botão, active escurece,
foco do TextInput troca a borda pra azul, Select realça a borda no hover e o
menu abre normalmente, disabled desliga a interação nos 4 widgets.

### 8. `@media` (responsivo)  ✅ (0.9.0)
**Problema.** Sem media queries; layout não reage à largura da janela.
**Fix (feito).**
- Parse de `@media (min/max-width|height: N) { .a {…} .b {…} }` — chave de
  fechamento casada por profundidade (`split_balanced_block`), condição por
  `parse_media_condition` (features `()` com AND; `and`/`screen` ignorados; `px`
  opcional). `StyleSheet.media: Vec<MediaQuery>`.
- `resolve_classes(classes, sheets, viewport)`: 2 passos — base, depois as
  `@media` cuja `MediaCondition::matches(w,h)` casa, POR CIMA da base.
- Viewport plumbado via `StyleContext.viewport`; o motor guarda
  `GlacierUI.viewport` (default 1280×800), atualizado por
  `EngineMessage::Viewport` que o listener `viewport_from_event` (em
  `subscription()`) emite no `window::Event::Resized`. `dispatch` só re-avalia
  se `media_set_changes` (cruzou breakpoint) — apps sem `@media` nunca re-avaliam.
**Arquivos.** `stylesheet.rs` (parse+resolve+3 testes), `eval.rs`
(StyleContext.viewport), `lib.rs` (viewport state, dispatch, subscription),
`widget.rs` (EngineMessage::Viewport). 100 testes; exemplo roda com `@media` sem
erro. **Toggle no resize precisa de verificação manual** (não observável headless,
como drag/Tab). Turnkey: quem usa `motor.subscription()` já recebe o listener.

---

## Tier 4 — Situacional / menor ratio (fazer sob demanda)

### 9. Seletores compostos/descendentes + especificidade + `!important`
`.a.b`, `.a .b`, `.a > .b`, `+`/`~`, modelo de especificidade e `!important`.
Grande retrabalho do matcher (hoje o seletor inteiro vira o nome da classe) para
ganho limitado dado o modelo de componentes. Avaliar só se a dor aparecer.

### 10. Propriedades extras pontuais
`opacity`, `box-shadow` (hoje sombra de botão é `Shadow::default()`), borda
**por-lado/por-canto** (hoje raio/leargura/cor únicos, sempre sólida),
`gradient` **radial/cônico** (hoje só linear), `transform`/`rotate`,
`line-height`/`letter-spacing`. Adicionar item a item quando um caso real pedir;
cada um é um campo novo em `StyleRule` + aplicação no `widget.rs`.

### 11. Seletor de id (`#nome`) + `#nome:estado`  ✅
**Problema.** Só havia seletor de classe. Estilizar *um* elemento específico
exigia inventar uma classe descartável, e não havia nível de especificidade
acima da classe.
**Fix (feito).**
- `#nome { }` e `#nome:estado { }` parseados em `parse_gss` (novo ramo, espelho
  do de classe), guardados em `StyleSheet.ids` / `StyleSheet.id_states` (e nos
  mesmos mapas dentro de `MediaQuery`) — nunca em `rules`/`states`.
- `UiNode.id: Option<String>` (attr `id`/`identificador`), interpolado no eval
  (`id="item-{i}"` funciona) e passado a `resolve_classes`/`resolve_state_classes`,
  que ganharam o parâmetro `id`. Resolução agora é **por tier de especificidade**:
  classe (base→`@media`) e depois id (base→`@media`) por cima — então um `#id`
  fora de `@media` vence uma `.classe` dentro de `@media`. Inline (no eval) vence
  o id. Cross-sheet e `var()` como as classes.
- `widget.rs` intacto: os overlays `#id:estado` entram no mesmo `StateStyles` que
  os widgets já consomem.
**Arquivos.** `stylesheet.rs` (ids/id_states + parse + resolve + 8 testes),
`parser.rs` (`UiNode.id` + attr), `eval.rs` (resolve quando `class` OU `id`).
Extensão VS Code (`gss.tmLanguage.json`/snippets/README) e exemplo `estilos`
(`#hero` vence `.title`) atualizados. **Exemplo rodado sem erro** (`cargo run
--example estilos`; render visual não conferido headless, como os demais).

---

## Convenções ao mexer

- Toda nova propriedade toca 4 pontos: `StyleRule` (campo + `merge_from`),
  `parse_rule_body` (chave→campo), `parser.rs` (attr inline), `eval.rs`
  (`resolve(&node.x, &style.x)`), `widget.rs` (aplicação). Seguir o padrão do
  `cursor` como referência de "propriedade completa ponta-a-ponta".
- Todo item ganha teste em `stylesheet.rs` (`#[cfg(test)]`) e, quando visual,
  um `examples/` rodado antes de publicar (ver memória "rodar exemplo antes de
  dar por pronto").
- Atualizar este arquivo (coluna Status) e o `README.md`/`ROADMAP.md` a cada
  item concluído.
