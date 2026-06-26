# xml-ui

Um motor de UI declarativa para Rust: você descreve a interface em **XML** e o
motor a renderiza com [`iced`](https://iced.rs), com **hot-reload**, **data
binding**, **componentes** reutilizáveis e **comportamento** encapsulado em tipos
Rust (ou embutido no próprio XML via `<script>`).

```xml
<Container padding="20" alignX="Center" alignY="Center" width="fill" height="fill" background="#2E3440">
    <Column spacing="20" align="Center">
        <Text content="Valor do Contador: {contador}" size="28" bold="true" color="#ECEFF4" />
        <Row spacing="15" align="Center">
            <Button text="Diminuir" onClick="decrementar" color="#BF616A" padding="10 20" />
            <Button text="Aumentar" onClick="incrementar" color="#A3BE8C" padding="10 20" />
        </Row>
    </Column>
</Container>
```

---

## Sumário

- [Instalação](#instalação)
- [Conceitos](#conceitos)
- [Início rápido](#início-rápido)
- [Referência de tags](#referência-de-tags)
- [Atributos de layout e estilo](#atributos-de-layout-e-estilo)
- [Data binding e templating](#data-binding-e-templating)
- [Controle de fluxo: `If`/`Else` e `ForEach`](#controle-de-fluxo)
- [Componentes e imports](#componentes-e-imports)
- [O trait `Component`](#o-trait-component)
- [Componentes aninhados](#componentes-aninhados)
- [`<script>` + a macro `#[component]`](#script--a-macro-component)
- [`ContextVar`](#contextvar)
- [Navegação entre telas](#navegação-entre-telas)
- [Hot-reload](#hot-reload)
- [Referência da API do motor](#referência-da-api-do-motor)
- [Exemplos](#exemplos)

---

## Instalação

O projeto é um workspace com dois crates:

- `xml-ui` — o motor.
- `xml-ui-macros` — a proc-macro `#[component]`.

Dependências (já no `Cargo.toml`): `iced 0.13`, `roxmltree`, `image`,
`serde_json`.

Rode qualquer exemplo com:

```bash
cargo run --example contador
```

---

## Conceitos

| Peça | Papel |
|---|---|
| **Template XML** | descreve a árvore de UI (layout, texto, botões, …). |
| **Contexto** (`context_data`) | mapa `String -> String` com o estado; templates leem dele via `{chave}`. |
| **`UiEngine`** | registra templates/componentes, avalia o contexto e renderiza para `iced`. |
| **`Component`** | tipo Rust que junta **UI** (template) + **comportamento** (reação às ações) + **estado** próprio. |
| **`EngineMessage`** | mensagens que o `iced` entrega ao motor (cliques, inputs, navegação, reload). |

O fluxo é: o XML é **parseado** → **avaliado** contra o contexto (placeholders,
includes, condicionais, loops) → **renderizado** em widgets `iced`. Ações da UI
viram `EngineMessage`, que o motor roteia ao `Component` dono.

---

## Início rápido

```rust
use xml_ui::{UiEngine, EngineMessage, Component, Context, Template};
use iced::{Element, Task, widget::text, Color};

struct Contador { valor: i32 }

impl Component for Contador {
    fn name(&self) -> &str { "contador" }
    fn template(&self) -> Template { Template::File("templates/contador.xml".into()) }

    fn init(&mut self, ctx: &mut Context) {
        ctx.set("contador", self.valor.to_string());
    }

    fn update(&mut self, action: &str, _value: Option<&str>, ctx: &mut Context) {
        match action {
            "incrementar" => self.valor += 1,
            "decrementar" => self.valor -= 1,
            _ => return,
        }
        ctx.set("contador", self.valor.to_string());
    }
}

struct App { motor: UiEngine }

impl App {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = UiEngine::new();
        motor.register(Box::new(Contador { valor: 0 })).unwrap();
        motor.set_initial_screen("contador");
        (Self { motor }, Task::none())
    }

    fn update(&mut self, msg: EngineMessage) -> Task<EngineMessage> {
        let _ = self.motor.dispatch(&msg);
        Task::none()
    }

    fn view(&self) -> Element<'_, EngineMessage> {
        self.motor.render_current()
            .unwrap_or_else(|e| text(e).color(Color::from_rgb(1.0, 0.0, 0.0)).into())
    }
}

fn main() -> iced::Result {
    iced::application("Contador", App::update, App::view)
        .run_with(|| App::new())
}
```

A app fica enxuta: registra o componente, repassa as mensagens para
`dispatch` e renderiza com `render_current`. Toda a lógica vive no `Component`.

---

## Referência de tags

Todas as tags aceitam variações de caixa e nomes em inglês **ou** português.

### Layout

| Tag | Aliases | Descrição |
|---|---|---|
| `<Container>` | `container` | caixa única (1 filho lógico); base para cartões/painéis. |
| `<Column>` | `column` | empilha os filhos verticalmente. |
| `<Row>` | `row` | dispõe os filhos horizontalmente. |

### Conteúdo e controles

| Tag | Aliases | Atributos próprios |
|---|---|---|
| `<Text>` | `text` | `content`/`texto`, `size`/`tamanho`, `bold`/`negrito`, `color`/`cor` |
| `<Button>` | `button`, `Botao` | `text`/`texto`, `onClick`/`aoClicar`, `navigateTo`/`irPara`, `navigateBack`/`voltar`, `color`/`cor` |
| `<TextInput>` | `Input`, `EntradaTexto` | `placeholder`/`dica`, `value`/`valor`, `onChange`/`aoMudar` |
| `<Image>` | `Imagem` | `source`/`src`/`caminho`, `clip="Circle"` (corte circular) |

### Estruturais (composição e fluxo)

| Tag | Aliases | Descrição |
|---|---|---|
| `<import>` | `Importar` | declara um componente carregado de um arquivo: `name`/`nome`, `from`/`de`. |
| `<Include>` | `Incluir` | inclui outro template inline; demais atributos viram props. |
| `<NomeDoComponente .../>` | — | qualquer tag desconhecida referencia um componente por nome; atributos viram props. |
| `<ForEach>` | `For` | repete os filhos por item: `items`/`itens`, `var`/`variavel`. |
| `<If>` | `Se` | renderiza condicionalmente: `cond`, `equals`, `notEquals`. |
| `<Else>` | `Senao` | renderiza quando o `<If>` imediatamente anterior foi falso. |

---

## Atributos de layout e estilo

Disponíveis em **qualquer** tag:

| Atributo | Aliases | Valores |
|---|---|---|
| `width` | `largura`, `w` | `fill`, `shrink` ou número (px) |
| `height` | `altura`, `h` | `fill`, `shrink` ou número (px) |
| `padding` | `espacamento_interno` | `"10"`, `"10 20"` (v h) ou `"10 20 30 40"` (t r b l) |
| `alignX` | `align_x`, `alinhamento_x` | `start`, `center`, `end` |
| `alignY` | `align_y`, `alinhamento_y` | `start`, `center`, `end` |
| `spacing` | `espacamento` | número (entre filhos de `Row`/`Column`) |
| `background` | `bg`, `fundo` | cor hex |
| `borderRadius` | `border_radius`, `raio_borda` | número |
| `borderWidth` | `border_width`, `largura_borda` | número |
| `borderColor` | `border_color`, `cor_borda` | cor hex |

**Cores:** hex `#RRGGBB` ou `#RRGGBBAA`.

---

## Data binding e templating

Qualquer valor de atributo pode conter placeholders `{chave}`, substituídos
pelos valores do contexto na avaliação:

```xml
<Text content="Olá, {user_name}!" color="{cor_texto}" />
```

O componente publica valores com `ctx.set("user_name", "Clara")`. Sempre que o
contexto muda (via `define_data`/`set`), o motor reavalia os templates e a UI
reflete o novo valor. Chaves ausentes viram string vazia.

---

## Controle de fluxo

### `If` / `Else`

`<If>` aceita três modos:

```xml
<If cond="{logado}">...</If>                 <!-- truthy: true/1/yes/on/sim -->
<If cond="{status}" equals="ativo">...</If>  <!-- comparação de igualdade -->
<If cond="{papel}" notEquals="admin">...</If><!-- comparação de diferença -->
<Else>...</Else>                             <!-- liga-se ao If anterior -->
```

### `ForEach`

Itera sobre um **array JSON** publicado no contexto. Cada item vira variáveis
prefixadas pelo nome declarado em `var`:

```xml
<ForEach items="usuarios" var="u">
    <CartaoUsuario nome="{u.nome}" cargo="{u.cargo}" cor="{u.cor}" />
</ForEach>
```

```rust
let json = serde_json::json!([
    { "nome": "Clara", "cargo": "Eng.", "cor": "#89B4FA" }
]).to_string();
ctx.set("usuarios", json); // objetos -> {u.nome}; strings simples -> {u}
```

---

## Componentes e imports

Há duas formas de compor UI **a nível de template**:

**1. `<import>` + referência por nome** (recomendado):

```xml
<import name="PerfilCard" from="templates/perfil_card.xml" />
<Container>
    <PerfilCard />   <!-- atributos viram props, ex.: <PerfilCard nome="{x}" /> -->
</Container>
```

**2. `<Include src="..." />`** — inclusão inline equivalente, com os demais
atributos como props.

Props são interpoladas no contexto local do componente incluído, então o filho
pode usar `{nome}`, `{cargo}` etc.

---

## O trait `Component`

Encapsula **UI + comportamento + estado** num único tipo Rust:

```rust
pub trait Component {
    fn name(&self) -> &str;                  // nome (registro + roteamento)
    fn template(&self) -> Template;          // Template::File(path) | Template::Inline(xml)
    fn init(&mut self, ctx: &mut Context) {} // estado inicial (opcional)
    fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context);
    fn children(&self) -> Vec<Box<dyn Component>> { Vec::new() } // sub-componentes
}
```

- `update` recebe a ação (`onClick`/`onChange`); `value` vem preenchido em
  inputs e é `None` em cliques.
- `Context` expõe `get`, `set`, `set_var`, `navigate_to`, `navigate_back`.
- O estado próprio mora nos campos do struct; o contexto guarda só a projeção
  string usada pelos templates.

Registre com `motor.register(Box::new(MeuComponente { .. }))`. O motor parseia o
template, chama `init` e passa a rotear ações para o `update`.

> Há também a API legada `register_component(name, path)`, que registra **só a
> UI** (o comportamento fica no `update()` do app). As duas convivem.

---

## Componentes aninhados

Um `Component` pode **possuir** outros via `children()`. Ao registrar o pai, o
motor registra os filhos em cascata (template + `init`), e as ações que saem da
UI de um filho são roteadas para o `update` **do filho**:

```rust
impl Component for Painel {
    fn name(&self) -> &str { "painel" }
    fn template(&self) -> Template { Template::File("templates/painel.xml".into()) }
    fn children(&self) -> Vec<Box<dyn Component>> {
        vec![Box::new(CartaoContador { valor: 0 })]
    }
    // ...
}
```

**Como o roteamento funciona:** ao inlinar a subárvore de
`<CartaoContador />`, o motor prefixa as ações daquela subárvore com o nome do
componente (`incrementar` → `CartaoContador::incrementar`). No `dispatch`:

- prefixo que corresponde a um componente com comportamento → vai para ele;
- ação sem prefixo, ou prefixo só de UI (sem `Component` registrado) → cai na
  **tela ativa** (fallback). É isso que mantém includes puramente visuais
  funcionando sem mudança.

> **Limite conhecido:** um filho referenciado N vezes (ex.: dentro de
> `<ForEach>`) compartilha um único objeto e um único `update` — ótimo para
> filhos sem estado próprio; estado por instância exigiria IDs de instância.

Veja `examples/aninhado.rs`.

---

## `<script>` + a macro `#[component]`

Dá para colocar o comportamento **dentro do próprio XML**, num bloco `<script>`
com métodos Rust. A macro `#[component]` lê o XML **em tempo de compilação**,
extrai o `<script>` e gera o `impl Component`.

```xml
<!-- templates/contador_macro.xml -->
<Container ...>
    <Text content="Valor: {contador}" />
    <Button text="+" onClick="incrementar" />
    <Button text="-" onClick="decrementar" />
</Container>

<script>
fn incrementar(&mut self) { self.contador += 1; }
fn decrementar(&mut self) { self.contador -= 1; }
</script>
```

```rust
use xml_ui::component;

#[component(path = "templates/contador_macro.xml", name = "contador")]
#[derive(Default)]
struct Contador { contador: i32 }
```

A macro gera:

- cada `fn` do `<script>` vira um método **e** uma ação de mesmo nome;
- um método com argumento extra recebe o valor do input (`fn mudar(&mut self, v: &str)`);
- cada campo nomeado do struct é sincronizado com o contexto (`to_string()`) no
  `init` e após cada ação — por isso `{contador}` reflete `self.contador`.

O `<script>` é removido **antes** do parse XML (`strip_script`), então pode ficar
como irmão da raiz, sem invalidar o documento.

**Tradeoff:** é Rust real, type-checado pelo compilador, mas mudar a lógica do
`<script>` exige **recompilar** (o markup continua com hot-reload). Veja
`examples/contador_macro.rs`.

---

## `ContextVar`

Açúcar para declarar variáveis de contexto de forma legível, em vez de chaves
string soltas:

```rust
fn init(&mut self, ctx: &mut Context) {
    let user_name = ContextVar::new("user_name", "Clara Silva");
    let btn_color = ContextVar::new("btn_color", "#313244");
    ctx.set_var(&user_name);
    ctx.set_var(&btn_color);
}
```

`ctx.set("chave", "valor")` (forma direta) continua disponível.

---

## Navegação entre telas

Cada tela é um componente registrado. Botões declaram o destino no próprio XML:

```xml
<Button text="Abrir perfil" navigateTo="perfil" />
<Button text="Voltar" navigateBack="true" />
```

O motor mantém uma pilha de histórico. No código:

```rust
motor.set_initial_screen("home"); // tela inicial, limpa o histórico
motor.navigate_to("perfil");      // empilha a atual
motor.navigate_back();            // volta à anterior
```

Componentes também podem pedir navegação de dentro do `update` via
`ctx.navigate_to(...)` / `ctx.navigate_back()`. Veja `examples/navegacao.rs`.

---

## Hot-reload

Os templates carregados de arquivo (`Template::File`) são recarregados quando
mudam em disco. Ligue a subscription do `iced` ao motor:

```rust
fn subscription(&self) -> iced::Subscription<EngineMessage> {
    UiEngine::reload_subscription(std::time::Duration::from_millis(500))
}

fn update(&mut self, msg: EngineMessage) -> Task<EngineMessage> {
    // dispatch já trata EngineMessage::FileChanged chamando check_reload()
    let _ = self.motor.dispatch(&msg);
    Task::none()
}
```

Edite o XML e veja a UI atualizar sem recompilar. (A lógica de um `<script>`
é a exceção — veja o tradeoff da macro acima.)

---

## Referência da API do motor

### `UiEngine`

| Método | Descrição |
|---|---|
| `new()` | cria um motor vazio. |
| `register(Box<dyn Component>)` | registra um componente (UI + comportamento + `children()` em cascata). |
| `register_component(name, path)` | API legada: registra só a UI a partir de um arquivo. |
| `dispatch(&EngineMessage)` | roteia a mensagem ao componente dono e aplica navegação/reload. |
| `set_initial_screen(name)` | define a tela ativa inicial e limpa o histórico. |
| `navigate_to(name)` / `navigate_back()` | navegação imperativa. |
| `render_current()` | renderiza a tela ativa para um `Element` do `iced`. |
| `render(name)` | renderiza um componente específico. |
| `define_data(k, v)` / `get_data(k)` / `get_data_mut(k)` | manipulam o contexto. |
| `reevaluate_all()` | reavalia todos os templates com o contexto atual. |
| `check_reload()` | recarrega arquivos alterados; retorna os nomes recarregados. |
| `reload_subscription(period)` | subscription do `iced` que dispara a checagem de reload. |

### `EngineMessage`

```rust
pub enum EngineMessage {
    XmlClick(String),                              // onClick
    XmlInputChanged { action: String, value: String }, // onChange
    Navigate(String),                              // navigateTo
    NavigateBack,                                  // navigateBack
    FileChanged(String),                           // tick do hot-reload
}
```

### Tipos de apoio

- `Template::File(String)` | `Template::Inline(String)`
- `Context` — `get`, `set`, `set_var`, `navigate_to`, `navigate_back`
- `ContextVar::new(key, value)`
- `Nav::To(String)` | `Nav::Back`

---

## Exemplos

| Exemplo | Demonstra |
|---|---|
| `contador` | `Component` básico com estado e cliques. |
| `contador_macro` | comportamento embutido no XML via `<script>` + `#[component]`. |
| `perfil` | inputs, cliques, `<import>` de um cartão e `ContextVar`. |
| `lista` | `<ForEach>` sobre JSON com um componente por item. |
| `condicional` | `<If>` / `<Else>` (truthy e comparação). |
| `navegacao` | múltiplas telas, histórico e `navigateTo`/`navigateBack`. |
| `aninhado` | componente registrado dentro de outro, com roteamento por namespace. |

```bash
cargo run --example aninhado
```
