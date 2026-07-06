# Known Issues / Limitações conhecidas

Bugs e limitações do glacier-ui já diagnosticados, para resolver no futuro.

---

## Diretiva (`if` / `else` / `ForEach`) como **nó raiz** de um componente é ignorada

**Sintoma.** Um componente cujo template tem uma diretiva condicional como
**nó raiz** — ex.:

```xml
<!-- loading_row.xml -->
<if cond="{flag}" equals="true">
  <Row class="loading_row"><Text content="Carregando…" /></Row>
</if>
```

renderiza os filhos **sempre**, independentemente da condição. No caso real que
motivou esta nota (um `LoadingRow flag="{data_loading}"`), o spinner "Carregando
dados…" nunca sumia mesmo depois de os dados chegarem e `data_loading` virar
`"false"` — enquanto o mesmo `if` usado *inline* (irmão do conteúdo) escondia/
mostrava corretamente.

**Causa raiz.** A avaliação de diretivas (`if`/`else`/`ForEach`) mora em
`expand_children` (`src/eval.rs`), que processa uma **lista de irmãos** e trata
`NodeType::If { cond, equals, not_equals }` avaliando a condição (por volta de
`eval.rs:431`, com o rastreio de `last_if` para o `else`). Mas `eval_owned`
— que avalia um **único nó** (a raiz de um componente é expandida por
`eval_owned` na referência do componente, `eval.rs:522`) — **não** avalia a
condição: quando o nó em si é `If`/`Else`/`ForEach`, ele cai no braço que
colapsa tudo para um `Container`:

```rust
// src/eval.rs (~631)
NodeType::Include { .. } | NodeType::Component { .. } | NodeType::Import { .. }
| NodeType::ForEach { .. } | NodeType::If { .. } | NodeType::Else
| NodeType::Link { .. } | NodeType::Style { .. } => {
    NodeType::Container
}
```

Ou seja: a diretiva-raiz vira um `Container` comum, **descartando a condição**
(e, no caso do `ForEach`, a iteração) e renderizando os filhos incondicional-
mente. Some-se a isso que o parser mantém um único nó de topo "como está"
(`src/parser.rs`, `parse_xml`: 1 root → mantém; N roots → `Fragment`), então um
componente com uma só diretiva no topo entrega exatamente esse `If`/`ForEach`
para o `eval_owned`.

> Observação: **não** é "props de componente não re-avaliam". As props são
> re-avaliadas a cada render (`eval.rs:516`, `process_template(val, context)` ao
> montar o `local_context`). A limitação é especificamente a diretiva-raiz.

**Workaround (atual).** Mover a diretiva para **fora** do componente, inline no
call site, onde `expand_children` a avalia normalmente:

```xml
<!-- call site -->
<if cond="{flag}" equals="true">
  <LoadingRow />
</if>
```

e deixar o template do componente com um nó "normal" na raiz (ex.: `Row`).
Alternativa equivalente: garantir que a diretiva **não** seja o nó raiz do
componente (envolvê-la num `Row`/`Column`/`Fragment` de topo — desde que o
`Fragment`/`Container` pai passe seus filhos por `expand_children`).

**Fix sugerido (raiz).** Em `eval_owned`, quando o nó for `If`/`Else`/`ForEach`,
roteá-lo pela mesma lógica de `expand_children` em vez de colapsar para
`Container`: tratar o corpo do componente como uma lista de irmãos (ex.:
expandir o(s) nó(s)-raiz via `expand_children` e devolver um `Fragment`), assim
a diretiva-raiz passa a respeitar a condição/iteração igual a uma diretiva
inline. Envolve um caso de teste com componente cujo root é `if`/`ForEach`.

**Contexto (rustploy).** Resolvido do lado do consumidor com o workaround inline
(sem tocar no glacier). Se um dia atacar na raiz aqui, é o fix acima +
publicação de nova versão do glacier-ui.
