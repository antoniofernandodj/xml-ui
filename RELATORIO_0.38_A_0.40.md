# Relatório: da 0.37.5 à 0.40.1 — erro tipado, diagnóstico, dirty-tracking e runner

Registro **honesto** do processo, incluindo o que tentei e não deu certo, as
hipóteses que a medição derrubou e os bugs que eu mesmo introduzi. O objetivo não
é celebrar o resultado, é deixar rastro de *por que* cada decisão foi tomada — e
sobretudo dos caminhos que **não** funcionaram, que é a parte que ninguém
consegue reconstruir lendo só o diff.

**Contexto.** O trabalho nasceu de uma pergunta do usuário ("o framework de
frontend deste projeto é robusto?"). A resposta apontou quatro pontos fracos;
ele mandou resolver os quatro. Depois, ao ler meu resumo, ele perguntou "mas
reavalia apenas os componentes sujos?" — e a resposta honesta era **não**, o que
abriu uma segunda rodada de trabalho.

---

## Mapa dos commits

### glacier-ui (`~/Development/rust/glacier-ui`)

| commit | versão | o que entrou |
|---|---|---|
| `3e586fb` | **0.38.0** | erro tipado, diagnóstico posicional, avaliação escopada, runner completo |
| `9decd1f` | **0.38.1** | fix: varredura de `<style>` fatiava `&str` por byte (pânico com `──`) |
| `d592def` | **0.39.0** | contexto em camadas (fim do clone do contexto por item) |
| `df91555` | — | chore: Cargo.lock |
| `c5d7a77` | **0.40.0** | dirty-tracking: rastreamento de leituras + cache de subárvores |
| `87a199a` | — | chore: Cargo.lock |
| `0090a27` | **0.40.1** | fix: var de item vazava para as dependências do template |
| `2b9eb8c` | — | docs: este relatório |
| `37b717f` | **0.41.0** | erro tipado até o Luau; `render_inputs` (invalidação via compilador); clippy 62→0; doc 5 links quebrados |
| `5347ac9` | — | style: rustfmt no repo inteiro (commit isolado) |
| `08fca4a` | — | docs: CHANGELOG.md + bump 0.41.0 |

Diffstat total (`f0361b5..HEAD`): **11 arquivos, +2764 / −581**. Os maiores:
`eval.rs` (+744), `lib.rs` (+735), `stylesheet.rs` (+650), `parser.rs` (+355),
`daemon.rs` (+301), `error.rs` (+322, novo).

### rustploy (`~/Development/rust/rustploy`)

| commit | o que entrou |
|---|---|
| `1d98462` | `app/mod.rs` deixa de ser um runtime `iced::daemon` à mão e vira configuração do `GlacierDaemon` (578 → 175 linhas) |
| `8da8a36` | sobe para glacier-ui 0.40.1; teste que trava o ganho da avaliação escopada |

---

## Parte 1 — Os quatro pontos (0.38.0)

### Ponto 1 — Erro tipado

**Decisão contra o roadmap.** O `ROADMAP.md` (e eu, no diagnóstico) diziam
"`thiserror`". Não usei. `Display`/`Error` à mão são ~40 linhas, evitam uma
dependência numa biblioteca e — o que pesou de verdade — me deram controle total
do formato da mensagem, que é o produto aqui. Registrei a divergência em vez de
seguir o plano no automático.

Campos do `GlacierUI` viraram privados com getters. Não é cerimônia: metade deles
são caches com invariantes acopladas (`stylesheets` e `stylesheet_paths` são
paralelos; a árvore avaliada precisa cair quando o contexto muda), e um `pub` em
cada um convida a quebrá-las de fora sem o compilador dizer nada.

### Ponto 2 — Diagnóstico

**Aqui eu quase consertei a coisa errada.** A memória do projeto dizia: *"nunca
escrever `<tag>` literal em comentário — erro aponta pra linha errada"*. Antes de
mexer, escrevi um repro (`examples/_repro_diag.rs`, temporário) e rodei. Resultado:

```
1) style c/ tag em comentário: Some("expected 'Text' tag, not 'style' at 5:3")
2) comentário XML c/ tag: None          ← funciona!
4) gss vírgula: None                     ← nenhum erro, falha silenciosa
```

Ou seja: **comentário XML com tag nunca foi o problema**. O culpado era
especificamente o corpo do `<style>`, que o parser de XML lia como XML — então
uma tag citada num comentário do *CSS* virava um elemento de verdade. Se eu
tivesse confiado na memória, teria mexido no tratamento de comentários (que está
correto) e não no `<style>` (que era o bug). **Lição: reproduzir antes de
consertar, mesmo quando existe uma nota dizendo qual é a causa.**

Três falhas silenciosas mortas:

1. **Corpo de `<style>` lido como XML** → agora blindado com CDATA
   (`protect_style_bodies`). Não introduz quebra de linha, então as posições
   sobrevivem.
2. **`strip_script` comia as linhas do bloco**, deslocando para cima todo erro
   abaixo dele → agora substitui o bloco por tantas quebras de linha quantas ele
   ocupava.
3. **`.a, .b { }` no GSS virava UMA classe de nome literal `"a, .b"`** — sem
   erro, sem aviso, sem estilo. Nenhum nó jamais casa essa classe. Era a falha
   mais cara do motor: o autor ia procurar o bug no XML. Lista por vírgula agora
   é suportada de verdade.

Também: comentário de bloco multi-linha deixou de deslocar linhas
(`strip_comments` preserva `\n`), e propriedade desconhecida avisa com
`arquivo:linha` e sugere a certa por distância de Levenshtein (`colr` →
*"você quis dizer 'color'?"*).

**Verificação visual.** Testes não avaliam mensagem de erro — mensagem se avalia
lendo. Rodei um exemplo que imprime os diagnósticos e li a saída. Foi lendo que
percebi uma inconsistência: a mensagem do `roxmltree` saía **em inglês** no meio
de um diagnóstico em português. Traduzi as variantes comuns (`xml_message`).

### Ponto 3 — "Dirty-tracking" (spoiler: não era)

Aqui está o erro mais importante deste relatório, e ele sobreviveu ao commit.

Entreguei em `3e586fb` uma mudança em `reevaluate_all` e **chamei de
dirty-tracking**. O que ela realmente faz é **escopo**: parar de avaliar *todo
template registrado* (cada um como raiz — e como avaliar inlina recursivamente os
componentes, um app com 15 componentes reconstruía a árvore inteira 16 vezes por
tecla digitada, 15 delas para árvores que ninguém renderiza). Isso é um corte
real e grande, mas **não é** "reavaliar só o que mudou": dentro da tela ativa,
tudo continuava sendo reconstruído do nó raiz à última folha.

Eu descrevi isso ao usuário com a etiqueta errada. Ele leu e perguntou: *"mas
reavalia apenas os componentes sujos?"*. A resposta era não. **Lição: a etiqueta
que eu dou ao meu próprio trabalho é uma afirmação técnica, e ela tem que ser
verdadeira.** A pergunta dele foi o que abriu a Parte 2.

### Ponto 4 — Runner

O `app/mod.rs` do rustploy era a prova do buraco: ~250 das suas 578 linhas eram um
runtime `iced::daemon` **reimplementado à mão** (roteamento por janela, listeners
globais, abertura de filhas, entrega de broadcasts), só porque o builder do
`GlacierDaemon` não expunha fontes embutidas, janela borderless, ícone, `min_size`
nem persistência de geometria.

O builder ganhou: `.font()`/`.default_font()`, `.main_window(Settings)`,
`.child_window(|spec, settings|)`, `.on_message()` (persistência),
`.on_close(WindowGeometry)`, `.reload_period()`, `.toast_period()`.

**Bônus não planejado:** as ações `window:*` estavam sendo resolvidas *dentro do
motor* via `window::latest()`. No Wayland esse round-trip perde o pointer-grab
serial e faz `window:drag` virar um no-op silencioso — o rustploy já contornava
isso na cópia local dele. Movi o tratamento para o runner, contra o `Id` da janela
em roteamento. O fix subiu para a lib e agora vale para qualquer app.

**Tropeço de compilação:** o `boot` do iced exige `Fn` (não `FnOnce`), então não
dá para *mover* os closures dos ganchos para dentro dele. Troquei `Box<dyn Fn>`
por `Rc<dyn Fn>` e clonei. Custo zero, cinco minutos perdidos.

### O bug que eu introduzi e os testes do rustploy pegaram (0.38.1)

Publiquei a 0.38.0, subi a dep no rustploy, rodei os testes de template — e três
falharam com pânico:

```
byte index 5 is not a char boundary; it is inside '─' (bytes 4..7)
  of `!-- ── Top edge + top corners (north / nw / ne) ──…`
```

Meu `find_style_open` fazia `&tail[..5]` para comparar o nome da tag. O texto
depois de um `<` pode começar no meio de um caractere multi-byte — e as views do
rustploy usam réguas `──` nos comentários de seção. Comparação passou a ser em
**bytes** (`eq_ignore_ascii_case(b"style")`).

**Observação:** esse bug passou por 209 testes do glacier e só apareceu no
primeiro consumidor real. Os templates de teste da lib são ASCII limpo; os do app
de verdade, não.

---

## Parte 2 — Dirty-tracking de verdade (0.39.0 → 0.40.1)

### Primeiro: medir. E a medição derrubou minha hipótese.

Antes de refatorar o coração da avaliação, montei uma bancada (temporária) contra
a **árvore real do rustploy**, não um exemplo sintético. Isso já rendeu três
tentativas erradas antes de eu conseguir medir a coisa certa:

1. Usei a chave `deployments` para a lista → **o `for-each` não expandiu**
   (86 nós antes e depois). A chave real era `eng_recent`.
2. Usei `view="deployments"` → a view certa era `deploy_engine`.
3. Ainda com 104 nós: a tabela estava atrás de um gate `data_loading`. Só depois
   de setá-lo é que a árvore foi de 104 para 600 nós.

Se eu tivesse aceitado o primeiro número, teria concluído "1 ms, está tudo bem" e
parado. **Lição: um benchmark que não mostra o crescimento esperado provavelmente
não está medindo o que você acha.**

Com a árvore certa:

| cenário | nós | por reavaliação |
|---|---|---|
| login | 46 | 0,4 ms |
| shell (tabela vazia) | 104 | 1,1 ms |
| shell + 45 linhas | 600 | **6,5 ms** |
| + log de 110 KB no contexto | 600 | **7,4 ms** |

~11 µs **por nó** — absurdamente alto. E o log de 110 KB somando 0,8 ms apontava
o dedo: cada item de `for-each` fazia `context.clone()`, copiando o contexto
inteiro (com a string de log dentro) por linha renderizada.

### Tentativa 1 (0.39.0): matar o clone — **e o ganho foi ridículo**

Troquei o `&HashMap` por um `EvalCtx` = base + cadeia de `Layer` (as vars do
item, as props do componente) encadeadas na pilha, sem copiar a base.

Resultado: **6,5 ms → 6,0 ms.**

A penalidade dos 110 KB sumiu (0,8 ms → 0,08 ms), confirmando que a análise do
clone estava certa — mas **o clone não era o gargalo**. Minha hipótese estava
errada. Os ~10 µs/nó restantes vêm de reconstruir o nó em si: resolver o estilo
dele (várias fusões de `StyleRule` de 21 campos, com clones de String) e montar um
`UiNode` de ~40 campos.

O commit ficou (é um ganho real e é pré-requisito do resto — a camada é justamente
o que dá identidade às instâncias para a chave de cache), mas sozinho não resolvia
nada.

### A medição que decidiu o desenho

Antes de escolher a estratégia, medi o que faltava saber: **quanto custa reusar
uma subárvore** (ou seja, cloná-la) em vez de reconstruí-la?

```
avaliar 600 nós: 6,3 ms   → 10,5 µs/nó
clonar 600 nós:  0,45 ms  →  0,75 µs/nó   ← 14× mais barato
```

É essa razão que faz a memoização valer a pena. Sem esse número eu estaria
chutando.

### Tentativa 2 (0.40.0): rastreamento + cache

- **`EvalCtx::get` é o único caminho de leitura** do contexto e registra toda
  chave lida. A completude é garantida pelo **compilador**: trocar o `&HashMap`
  pelo wrapper obrigou cada call-site a mudar. Isso não é preciosismo — uma
  leitura esquecida não daria erro, daria **UI silenciosamente velha**, que é o
  pior bug possível num motor de UI (silencioso, intermitente, impossível de
  atribuir). Foi por isso que rejeitei a alternativa mais barata (análise estática
  dos `{placeholder}` do template): ela *poderia* esquecer um caso.
- **Quadros aninhados de leitura**: ao fechar um quadro, as leituras sobem para o
  de fora — uma chave lida no fundo de uma subárvore também é dependência de todos
  os ancestrais dela.
- **`EvalCache`** memoiza nas duas fronteiras que pagam: o uso de um **componente**
  (props bem definidas — é o que cobre a sidebar, feita de `<NavItem>`) e cada
  **item de for-each**. Chave = hash do caminho (`node_id` + índice), daí o campo
  novo `UiNode.node_id`.
- **`reevaluate_all` nem entra na árvore** se nada que a tela *lê* mudou.

### A armadilha central (e por que ela tem teste próprio)

O rastreamento só enxerga **chaves de contexto**. Três coisas mudam a árvore sem
passar por ele: folha de estilo recarregada, viewport cruzando um `@media`, e
markup reparseado. Se o cache não fosse invalidado nesses casos, ele serviria nós
com o **estilo velho**. São 8 pontos de `invalidate_eval_cache()`, todos
comentados, e há um teste dedicado (`estilo_novo_invalida_o_cache`) — porque é o
erro mais fácil de cometer neste desenho e ele não quebra nada visível na hora.

**Listas reordenáveis ficam fora do cache de propósito:** o corpo de cada item
carrega `drag_order` (a ordem inteira da lista) **injetado**, não lido do contexto
— então o rastreamento não teria como perceber que a ordem mudou, e serviria o
item com a ordem velha. São listas pequenas (env vars); reavaliá-las sempre é de
graça. Preferi excluí-las a inventar um remendo.

### O bug que só a medição pegou (0.40.1)

Com 209 testes verdes e o cache pronto, medi de novo:

```
login:        400 µs → 0,7 µs    ← pula a reavaliação
shell vazio:  1,1 ms → 0,68 ms   ← NÃO pula
600 nós:      6,3 ms → 5,1 ms    ← mal melhorou
```

O login pulava, o shell não. Diferença entre os dois: **o shell tem listas.**

Causa: as variáveis de item (`{l.nome}`) só existem na camada daquele item, mas
estavam subindo até o conjunto de dependências do **template**. Lá em cima o motor
pergunta *"o contexto ainda tem `l.nome` = a?"* — e a resposta é **sempre não**,
porque `l.nome` nunca esteve no contexto. Resultado: toda tela com uma lista ficava
**eternamente suja**, e o cache existia sem nunca acertar.

Nenhum teste pegaria isso: o resultado renderizado estava **correto**, só era
lento. Foi a medição, e só ela.

Correção: cada leitura registra também a **profundidade da camada** que a resolveu;
ao fechar um quadro de profundidade `d`, só sobem as leituras resolvidas *fora*
dele (`src < d`). A var de item fica onde importa (a validade do cache daquele
item, checada contra a camada dele) e não contamina quem está por fora.

### Resultado final (árvore real do rustploy, 600 nós)

| cenário | antes | depois |
|---|---|---|
| muda uma chave que **ninguém** lê (snapshot de view fechada) | 6,3 ms | **3,5 µs** |
| muda uma chave **lida**, tabela de 45 linhas intacta | 6,3 ms | **1,6 ms** |

O segundo é o caso do log ao vivo: o cabeçalho é reconstruído; a tabela ao lado e
a sidebar são reaproveitadas.

---

## Observações que valem para a próxima vez

1. **Reproduzir antes de consertar.** A nota do projeto sobre "tag em comentário"
   estava parcialmente errada. Um repro de 10 linhas apontou o culpado real
   (`<style>`) e me impediu de mexer no código certo.

2. **Medir antes de otimizar — e desconfiar de um benchmark que não cresce.**
   Errei a chave da lista, a view e um gate booleano antes de medir a árvore
   certa. Um número plausível mas errado (1 ms) quase encerrou a investigação.

3. **A hipótese óbvia estava errada.** O `context.clone()` por item *parecia* o
   gargalo e não era (6,5 → 6,0 ms). Só a medição do custo de *clonar* vs
   *avaliar* (14×) revelou onde estava o dinheiro.

4. **Testes verdes não bastam para performance nem para mensagens de erro.** O bug
   do vazamento da var de item passava em 209 testes (o resultado era correto, só
   lento). A inconsistência de idioma nos diagnósticos só apareceu lendo a saída.

5. **O primeiro consumidor real acha o que a suíte não acha.** O pânico do
   `&tail[..5]` só apareceu nos templates do rustploy, que usam `──` nos
   comentários.

6. **Etiquetar o próprio trabalho é uma afirmação técnica.** Chamei escopo de
   dirty-tracking; a pergunta do usuário expôs isso. Se ele não tivesse
   perguntado, a diferença teria ficado enterrada num commit.

---

## Parte 3 — Fechando a conta (0.41.0)

Depois de tudo publicado, o usuário perguntou se a lib era **de fato** robusta.
Medi de novo os mesmos indicadores da primeira avaliação, e a resposta honesta era
"para o rustploy sim; como biblioteca pública, ainda não" — faltava o cinto de
segurança. Ele mandou fechar tudo.

### O erro tipado estava pela metade

`LuauComponent::from_file`/`from_source` são **públicos e reexportados**, e ainda
devolviam `Result<_, String>`. Eu embrulhava no `lib.rs`, então quem usava o motor
via o tipo certo — mas quem usasse a API do Luau direto, não. Agora devolvem
`GlacierError::Luau`. Por dentro a construção segue com `String`, e isso é
deliberado: as mensagens vêm do mlua e já dizem arquivo e linha **do Luau**, que é
a informação que importa. O tipo entra onde tem valor: no contrato público.

### A dívida que eu mesmo criei: a invalidação do cache

Era o ponto mais importante desta rodada, e eu o tinha **nomeado como dívida** no
commit da 0.40: o cache só enxerga chaves de contexto, então folha de estilo,
viewport e markup precisavam de oito chamadas manuais de
`invalidate_eval_cache()`. Funcionava — e era exatamente o tipo de invariante que
sobrevive à revisão e morre seis meses depois.

Pior: ao ir consertar, encontrei **uma delas já furada**. O hot-reload de `.gss`
escrevia direto em `stylesheets[idx]`, sem passar pelo ponto de invalidação; só
não servia estilo velho porque um `invalidate` genérico vinha depois, por sorte.
A bomba já estava armada.

Novo módulo `render_inputs.rs`: folhas, templates e viewport viram campos
**privados** noutro módulo, e toda mutação passa por um método que incrementa uma
`epoch`. O cache guarda a época em que foi construído e se descarta sozinho. A
invariante saiu das minhas mãos e foi para as do compilador — que, ao remover os
campos, acusou os **36 call-sites** de uma vez, um inventário que eu não teria
conseguido montar sozinho.

Detalhe que só apareceu ao escrever o teste: `set_viewport` **não pode** avançar a
época em todo resize. Arrastar a borda da janela emitiria um `Resized` por pixel, e
cada um jogaria fora o cache inteiro — a "otimização" ficaria mais lenta que não
ter cache. Só avança se o resize **cruza** um breakpoint de `@media`.

### Clippy, doc, CI, CHANGELOG

- **Clippy 62 → 0**, com `-D warnings` passando.
- **`cargo doc -D warnings`** achou **5 links quebrados de verdade** na
  documentação — inclusive um `EngineMessage::LuaStream` que **não existe** (é
  `LuauStream`) e dois links de doc pública apontando para itens privados. O gate
  pagou por si antes mesmo de rodar em CI.
- **rustfmt no repo inteiro** (32 arquivos). Commit **isolado**: reformatação
  misturada a lógica é uma revisão impossível.
- **CI** (GitHub Actions): build, testes, clippy `-D warnings`, `fmt --check`,
  `cargo doc -D warnings`.
- **CHANGELOG.md** cobrindo 0.38.0 → 0.41.0, com as quebras e como migrar. Foram
  três quebras de API num dia; sem isso, quem não é o autor descobre na compilação.

### Observação de método

Quase apaguei todo o trabalho não commitado: fui rodar `cargo fmt` e reverter com
`git checkout -- .` para *medir* o tamanho do diff, com 16 arquivos sujos na
árvore. O guarda-corpo do ambiente barrou. A forma certa era a óbvia: **commitar
primeiro, formatar depois** — que é o que acabou virando dois commits limpos.

---

## O que ficou pendente

- **Nós fora das fronteiras memoizadas** (o cromo do shell, ~105 nós) ainda são
  reconstruídos — é o 1,6 ms residual. Memoizar subárvores arbitrárias exigiria
  identidade estável para todo nó, não só componentes e itens.
- **Verificação interativa**: drag-and-drop, janela filha e persistência de
  geometria não foram exercitados (a sessão é Wayland; só há `import`, que é X11).
  O caminho de código é o mesmo de antes, mas quem confirma é abrir o app.
- **`deny(missing_docs)`** segue pendente (ver `ROADMAP.md`).
- **Nenhum benchmark no repositório.** Os números deste relatório vieram de uma
  bancada temporária que foi removida. Há testes travando o *comportamento* (uma
  chave não lida não reconstrói a árvore), mas nada travando o *custo* — uma
  regressão de performance passaria batida pelo CI.


next: "agora resolve o 1,6ms residual memoizando subárvores arbitrárias"
