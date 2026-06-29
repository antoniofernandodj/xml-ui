# Plano: condicionais/loops como atributos (não tags)

> Status: **planejado, não iniciado.** Escrito em 2026-06-29 ao fim de uma sessão.
> Retomar amanhã com mais tokens. Tudo abaixo já foi estudado e decidido.

## Objetivo

Hoje `<if>`, `<else>` e `<ForEach>` são **tags-invólucro** (variantes do enum
`NodeType`). Queremos que a forma **primária e recomendada** vire **atributo em
qualquer elemento**, no modelo Vue/Angular:

```xml
<!-- hoje -->                          <!-- novo (alvo) -->
<if cond="{logado}">...</if>           <Column if="{logado}">...</Column>
<else>...</else>                       <Column else>...</Column>
<ForEach items="usuarios" var="u">     <CartaoUsuario for-each="usuarios" var="u"
  <CartaoUsuario nome="{u.nome}"/>                     nome="{u.nome}" .../>
</ForEach>
```

O `for-each` como atributo fica especialmente melhor: o elemento repetido carrega
o loop diretamente.

## Decisões já tomadas (NÃO reabrir)

1. **`else` pelado** (`<Column else>`), apesar de XML estrito não aceitar atributo
   sem valor. Resolvido por uma **passada de pré-processamento de string** que
   reescreve `else` pelado → `else=""` **antes** do `roxmltree` parsear — mesmo
   padrão do `strip_script` (eval.rs:12-30).
2. **Retrocompat: manter as tags `<if>/<else>/<ForEach>` como açúcar.** NÃO apagar
   as variantes `NodeType::If/Else/ForEach`. Marcar como legado com `TODO` (regra
   do usuário: comentar/deixar com TODO, não remover o reutilizável). O engine
   passa a suportar AS DUAS formas simultaneamente.
3. **Precedência quando `for-each` + `if` no mesmo elemento:** `for-each` por fora
   (desenrola primeiro), `if` filtra por item.
4. Comunicação e comentários em **pt-BR**.

> Honestidade importante: com retrocompat, o código do engine **cresce um pouco**
> agora (suporta 2 formas). A simplificação real é na **autoria** (markup mais
> enxuto). A remoção das variantes legadas fica para um passo futuro, quando os
> templates forem migrados.

## Arquitetura atual (mapa do terreno)

- **parser.rs**
  - `enum NodeType` (linhas ~10-115): variantes `If { cond, equals, not_equals }`,
    `Else`, `ForEach { items, var }` em ~88-103.
  - `struct UiNode` (~118-148): campos planos `Option<String>` para estilo/layout
    (`width`, `class`, etc.). É aqui que entram os novos campos de diretiva.
  - `fn from_node` (~149-330): match na tag → `NodeType`. Arms de `If`/`Else`/
    `ForEach` em ~tag "If"/"Else"/"ForEach". Atributos lidos via helper
    `get_attr(node, &[chaves...])` (case/idioma-insensitive).
  - `fn parse_xml` (~349): embrulha em `<__glacier_fragment__>` e separa decls.
- **eval.rs**
  - `strip_script` (12-30): **modelo a copiar** para a passada de normalização.
  - `process_template` (33-64): troca `{chave}` pelo contexto.
  - `eval_condition` (76-90): recebe `cond, equals, not_equals` — **reaproveitar**.
  - `expand_children` (124-203): **coração**. Percorre irmãos; trata `If` (eval +
    `last_if`), `Else` (liga ao `last_if==Some(false)`), `ForEach` (desenrola array
    JSON re-expandindo o corpo por item via `local_context`), dropa Import/Link.
    É aqui que entra o caminho novo por atributo.
  - `eval_owned` (235-402): avalia um nó. Tem um braço catch-all
    (349-353) que colapsa `If/Else/ForEach/Include/...` para `Container`. Constrói
    `UiNode` final em 384+ (onde os novos campos precisam ser zerados/default).
- **widget.rs**
  - Arms `NodeType::ForEach` (493-496) e `If | Else` (505-508): renderizam vazio.
    Permanecem (legado).
- **lib.rs** — pré-processamento do XML em 3 lugares (inserir normalize em todos):
  - 200-201 (registro), 348-349, 563-564 (hot-reload). Padrão:
    `let (markup, _script) = eval::strip_script(&xml); UiNode::parse_xml(&markup)`.
  - `pub use eval::{... strip_script ...}` em linha 8.
- **glacier-ui-macros/src/lib.rs:21** — espelha `strip_script` (macro não depende do
  engine). NÃO precisa do normalize (só usa o script), mas conferir.

## Passo a passo (ordem de execução)

### 1. Pré-processamento: `else` pelado → `else=""`
- Em **eval.rs**, junto de `strip_script`, criar
  `pub fn normalize_bare_directives(xml: &str) -> String`.
- Implementação robusta (NÃO regex ingênuo, senão pega "else" em texto/valores):
  tokenizar caractere a caractere, rastreando estado: dentro de tag (entre `<` e
  `>`) e fora de valor de atributo entre aspas. Quando, dentro de tag e fora de
  aspas, aparecer o token `else` isolado (delimitado por espaço/`<`/início e
  seguido por espaço/`/`/`>` mas **não** por `=`), reescrever para `else=""`.
- Aplicar em **lib.rs** nos 3 call sites, logo após `strip_script`:
  `let markup = eval::normalize_bare_directives(&markup);`
- Exportar em `pub use` (linha 8) se for usada fora.

### 2. Novos campos em `UiNode` (parser.rs)
```rust
// Diretivas estruturais como atributos (modelo Vue/Angular). Consumidas em
// expand_children; não renderizam por si.
pub if_cond: Option<String>,       // if="{cond}"
pub if_equals: Option<String>,     // equals="..."   (par do if)
pub if_not_equals: Option<String>, // notEquals="..."
pub is_else: bool,                 // else (pelado, normalizado p/ else="")
pub for_each: Option<String>,      // for-each="items"
pub for_each_var: Option<String>,  // var="u"
```
- Atualizar **todos** os locais que constroem `UiNode { ... }`:
  - `from_node` final (~316-330).
  - `eval_owned` final (eval.rs:384+) → setar tudo como `None`/`false` (diretiva já
    consumida; não deve vazar para a saída).

### 3. Ler as diretivas em `from_node` (parser.rs)
- Para **todo** nó (antes/depois dos atributos de estilo), ler:
  - `if_cond = get_attr(&node, &["if", "se"])`
  - `if_equals = get_attr(&node, &["equals", "eq", "igual_a"])`
  - `if_not_equals = get_attr(&node, &["notEquals", "not_equals", "ne", "diferente_de"])`
  - `is_else = node.has_attribute("else") || has_attribute("senao")`
  - `for_each = get_attr(&node, &["for-each", "forEach", "foreach", "each", "repeat"])`
  - `for_each_var = get_attr(&node, &["var", "variavel"])`
- ATENÇÃO ao conflito de tag: a tag `<For>`/`<ForEach>` legada usa `items`/`var`.
  Os nomes de atributo do caminho novo (`for-each`) são distintos de `items`, então
  não colidem. Manter as arms de tag legadas intactas.

### 4. `expand_children` — caminho novo por atributo (eval.rs)
No topo do loop, ANTES do `match &child.kind`, checar diretivas do nó:
```text
se child.for_each.is_some():
    avaliar nome via process_template; pegar array JSON do contexto.
    para cada item:
        montar local_context (igual ao ForEach atual: objeto => "var.campo",
            string/num => "var").
        clonar child com for_each=None/for_each_var=None;
        expand_children(slice_de_um(&clone), &local_context, ...)  // reusa if/else
    last_if = None; continue;
senão se child.is_else:
    se last_if == Some(false): empurrar eval_sem_diretiva(child)
    last_if = None; continue;
senão se let Some(cond) = &child.if_cond:
    truthy = eval_condition(cond, &child.if_equals, &child.if_not_equals, context)
    se truthy: empurrar eval_owned(child, ...)
    last_if = Some(truthy); continue;
// senão cai no match &child.kind existente (inclui legado If/Else/ForEach)
```
- `eval_owned(child,...)` já recursiona em `child.children` via `expand_children`,
  então os filhos são expandidos normalmente. As diretivas do próprio `child` já
  foram consumidas aqui e zeradas na saída (passo 2).
- O `else` dentro de um único item de for-each não cruza itens (slice de 1) — ok.

### 5. Manter legado e marcar TODO
- Deixar as arms `If`/`Else`/`ForEach` em `from_node`, o tratamento em
  `expand_children` (match kind) e os arms em `widget.rs` **como estão**.
- Adicionar comentário `// TODO(diretivas): forma legada por tag; preferir
  atributos if/else/for-each. Remover quando templates forem migrados.` em cada
  ponto legado.

### 6. Exemplos/templates
- NÃO migrar os existentes (retrocompat garante que funcionam).
- Criar **um** template novo demonstrando a forma por atributo (ex.:
  `templates/condicional_attr.xml`) e, se valer, um exemplo em `examples/`.
- Atualizar `README.md` documentando a nova sintaxe como recomendada e a precedência
  for-each+if. Mencionar `else` pelado.

### 7. Validar
- `cargo build` no glacier-ui.
- `cargo run --example condicional` e `--example lista` (formas legadas devem seguir
  funcionando).
- Rodar o exemplo novo com a forma por atributo.
- `cargo test` (tests/engine_tests.rs) — checar se há testes de if/foreach a
  estender para a forma de atributo.

## Armadilhas conhecidas
- **XML estrito**: `roxmltree 0.21` não aceita atributo pelado → daí o passo 1.
- **Normalização ingênua**: não usar replace global de "else"; respeitar aspas e
  fronteiras de tag (passo 1).
- **Vazamento de diretiva**: zerar os campos novos na saída de `eval_owned` (passo 2)
  senão a diretiva re-dispara.
- **Transparência**: a tag `<if>` legada é transparente (splica vários filhos como
  irmãos). A forma por atributo aplica a UM elemento; p/ múltiplos, embrulhar num
  `<Container if=...>`. Nos exemplos atuais, if/foreach sempre envolvem 1 filho.
- **Não confundir** com o projeto rustploy (consumidor do glacier-ui via crate
  remote-ui). Esta mudança é só no glacier-ui.
