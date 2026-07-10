# Widgets embutidos (`src/builtins.rs`)

Biblioteca de componentes que a própria `glacier-ui` registra sozinha, para
ficarem disponíveis por tag em **qualquer** template sem o app configurar nada —
o objetivo de longo prazo é uma biblioteca vasta de widgets, no espírito do Qt.

Este documento é o guia prático de **como estender** essa biblioteca. As
garantias e restrições do motor por trás dela estão documentadas nas docstrings
de `src/builtins.rs`; aqui o foco é o passo a passo.

## Três níveis de "componente" no glacier-ui

| | Onde vive | Precisa registrar? | Disponível como |
|---|---|---|---|
| **Primitiva** | `src/widget.rs` + `src/parser.rs` | não | `<Button/>`, `<Text/>`, … |
| **Builtin** | `src/builtins.rs` | não (a lib registra) | `<Badge/>` e afins |
| **Componente do app** | código/arquivos do app | sim (`register`/`import`) | `<PerfilCard/>` |

Um **builtin** é um componente comum (`impl Component`) — a única diferença é que
a lib o registra em `GlacierUI::new()`, então ele não exige `register()` do app.
Uma tag desconhecida no XML vira uma referência de componente resolvida pelo
nome; como o builtin já está registrado, `<Badge/>` "simplesmente funciona".

## Passo a passo: adicionar um widget

### 1. Escreva o `impl Component` em `src/builtins.rs`

```rust
/// Um separador horizontal fino — divide seções de uma coluna.
///
/// Props (opcionais, com default inline):
/// - `divider_color`  — cor. Default: `#313244`.
/// - `divider_height` — espessura em px (numérico). Default: `1`.
struct Divider;

impl Component for Divider {
    fn name(&self) -> &str {
        "Divider"
    }

    fn template(&self) -> Template {
        Template::Inline(
            r#"<Container
                    background="{divider_color|#313244}"
                    width="fill"
                    height="{divider_height|1}"
                />"#
                .to_string(),
        )
    }

    fn update(&mut self, _action: &str, _value: Option<&str>, _ctx: &mut Context) {}
}
```

### 2. Registre-o na lista

`builtin_components()` é o **único** ponto que o motor lê. Some o widget lá:

```rust
pub fn builtin_components() -> Vec<Box<dyn Component>> {
    vec![
        Box::new(Badge),
        Box::new(Divider), // <-- novo
    ]
}
```

Pronto. `<Divider/>` já está disponível em qualquer template. Nada mais na lib
precisa mudar.

### 3. Teste (ver a seção _Testando_)

## O contrato `Component`

Definido em `src/component.rs`. Para um widget apresentacional você só implementa
`name`, `template` e um `update` vazio; o resto tem default.

| Método | Obrigatório | Papel |
|---|---|---|
| `name(&self) -> &str` | sim | Nome único = a tag usada no XML (`"Badge"` → `<Badge/>`). |
| `template(&self) -> Template` | sim | A UI. Em builtin, **sempre** `Template::Inline` (ver abaixo). |
| `update(&mut self, action, value, ctx)` | sim | Reage a ações da própria UI. Vazio se apresentacional. |
| `init(&mut self, ctx)` | não | Semeia estado inicial. **Evite em builtins** (ver restrições). |
| `children(&self) -> Vec<Box<dyn Component>>` | não | Subcomponentes próprios, registrados em cascata. |
| `on_form_submit(&mut self, action, ctx)` | não | Trata o `onSubmit` de um `<Form>`. |

### Por que `Template::Inline` e não `Template::File`

O XML de um builtin é **compilado no binário** (uma string `Inline`), nunca lido
de disco. Isso torna o parse determinístico: se falhar, é bug da lib, não do app.
Graças a isso `register_builtins` pode usar `expect`/`panic`, e `GlacierUI::new()`
continua infalível (devolve `Self`, não `Result`). Um builtin com
`Template::File` reintroduziria I/O que pode falhar em runtime — não use.

Dica de sintaxe: como o XML tem `#` (cores) logo após aspas (`background="#..."`),
use raw string com `##` quando houver cores literais: `r##"..."##`. Com só
placeholders (`{badge_bg|#...}`) o `#` está "dentro" do template e `r#"..."#`
basta — mas `r##` sempre funciona.

## Props: passar valores por instância

Os atributos de `<Badge foo="x"/>` viram **props**: são mesclados num clone do
contexto *só daquela instância* durante a avaliação. No template, `{foo}` resolve
para o valor da prop.

```xml
<Badge badge_text="Novo" badge_bg="#A6E3A1" />
```
```rust
// no template do Badge:
content="{badge_text}"  background="{badge_bg}"
```

Convenção: **prefixe** os nomes de prop com o do widget (`badge_*`, `divider_*`).
Não é obrigatório, mas evita confusão com chaves de contexto do app.

### Defaults por instância: `{prop|default}`

Um placeholder pode declarar um default inline após `|`. Se a prop não for
passada, usa-se o texto depois do `|`:

```xml
background="{badge_bg|#89B4FA}"   <!-- sem a prop badge_bg, fica #89B4FA -->
```

Isso é o jeito **correto** de dar defaults a um builtin — não semeie defaults no
contexto global via `init()` (ver restrições). Espaços em volta da chave e do
default são aparados: `{ badge_bg | #89B4FA }` funciona.

### Atributos numéricos também aceitam props

Atributos numéricos — `size`, `spacing`, `border_radius`, `border_width`,
`max_width`, `max_height` — aceitam `{prop}` (e `{prop|default}`), igual aos de
string. O motor detecta o `{`, adia a conversão e resolve para `f32` na
avaliação (mecanismo em `src/parser.rs`, enum `NumAttr`).

```xml
<Text content="{badge_text|Badge}" size="{badge_size|13}" />
```

Se o valor resolvido não for um número (ex.: prop omitida e sem default), o
atributo fica sem valor — herda o default do widget nativo do `iced`.

### Precedência de um campo

Do mais forte ao mais fraco:

1. Valor templado (`{…}` já resolvido)
2. Literal inline no atributo (`padding="4 10"`)
3. Valor herdado de uma classe `.gss` (`class="..."`)

## Espaço de nomes e override

Builtins compartilham o espaço de nomes dos componentes do app. Para não
"sequestrar" um nome, **uma definição explícita do app vence o builtin** de mesmo
nome:

- `register(Box::new(MeuBadge))` ou `register_component("Badge", …)` inserem por
  cima.
- `<import name="Badge" from="…"/>` também sobrescreve (o guarda de imports abre
  exceção para nomes que ainda são builtin).

Ou seja: escolha nomes bons, mas saiba que o app sempre pode substituir. Evite
colidir com **primitivas** (`Button`, `Text`, `Column`, `Row`, `Container`,
`Image`, `Svg`, `Checkbox`, `Toggle`, `Select`, `Form`, `Rule`, …) — essas são
resolvidas antes e não são sobrescrevíveis por um componente.

## Restrição importante: contexto global único

O estado escrito com `ctx.set` (em `update` ou `init`) vai para **um** contexto
global — não há estado por instância. Consequências práticas:

- ✅ **Widgets apresentacionais / prop-driven** (Badge, Divider, Avatar, Card):
  recebem tudo por prop, não guardam estado. Podem ser usados N vezes na mesma
  tela sem colisão. **É o tipo recomendado hoje.**
- ⚠️ **Widgets com estado** (um contador, um accordion aberto/fechado): duas
  instâncias na mesma tela **compartilhariam** o estado e colidiriam. Não há
  isolamento por instância ainda.
- ❌ **Não** semeie defaults com `init()` num builtin: isso polui o contexto
  global com as chaves do widget. Use `{prop|default}` no template.

Enquanto o motor não tiver estado por instância, mantenha os builtins
apresentacionais.

## Testando

Um teste de integração exercita o caminho completo (parse → builtin
auto-registrado → árvore avaliada) sem GUI. Registre **só** uma tela que usa a
tag e verifique a árvore em `motor.evaluated_templates`:

```rust
#[test]
fn test_divider_disponivel_sem_registro() {
    let mut motor = GlacierUI::new();
    std::fs::create_dir_all("templates").ok();
    let tela = "templates/test_divider.gv";
    std::fs::write(tela, r##"<Column><Divider divider_color="#f00" /></Column>"##).unwrap();

    motor.register_component("tela", tela).unwrap(); // NÃO registra Divider

    let ev = motor.evaluated_templates.get("tela").unwrap();
    assert_eq!(ev.children[0].background.as_deref(), Some("#f00"));
    // default inline preservado onde a prop foi omitida:
    // assert_eq!(ev.children[0].height.as_deref(), Some("1"));

    std::fs::remove_file(tela).ok();
}
```

Veja `tests/engine_tests.rs`:
- `test_builtin_badge_disponivel_sem_registro` — disponibilidade sem registro + defaults + não-poluição do contexto.
- `test_atributo_numerico_templado` — prop num atributo numérico.
- `test_template_default_inline` — a sintaxe `{prop|default}`.

## Checklist

- [ ] `name()` único, sem colidir com primitivas.
- [ ] `Template::Inline` (nunca `File`).
- [ ] Props prefixadas (`widget_*`) com default inline `{prop|default}`.
- [ ] Sem `init()` semeando contexto global; sem estado se for usável N vezes.
- [ ] Adicionado a `builtin_components()`.
- [ ] Docstring no `struct` listando as props e seus defaults.
- [ ] Teste de disponibilidade-sem-registro em `tests/engine_tests.rs`.

## Referência: o `Badge`

`Badge` é o exemplo canônico — uma "pílula" de rótulo, puramente apresentacional,
com props string e numérica, todas com default inline. Veja o código-fonte em
`src/builtins.rs` e o exemplo executável em `examples/builtins/` (`cargo run
--example builtins`).
