# glacier-ui

**Glacier** é um motor de UI declarativa para Rust: você descreve a interface em
**XML** e o motor a renderiza com [`iced`](https://iced.rs). O comportamento pode
morar em **Rust** (o trait `Component`) ou **dentro do próprio template**, num
bloco `<script>` em **[Luau](https://luau.org)** interpretado em tempo de execução
— com **hot-reload**, **data binding**, **componentes**, **navegação**,
**formulários reativos**, **stylesheets `.gss`** (CSS-like), **rede assíncrona**
(`fetch`/SSE/WebSocket), **toasts** e **diálogos**.

```xml
<!-- examples/contador/contador.gv -->
<Container padding="20" alignX="Center" alignY="Center" width="fill" height="fill" background="#2E3440">
    <Column spacing="20" align="Center">
        <Text content="Valor do Contador: {contador}" size="28" bold="true" color="#ECEFF4" />
        <Row spacing="15" align="Center">
            <Button text="Diminuir" on_click="decrementar" color="#BF616A" padding="10 20" />
            <Button text="Aumentar" on_click="incrementar" color="#A3BE8C" padding="10 20" />
        </Row>
    </Column>
</Container>
```

Duas formas de dar comportamento a esse XML — escolha por caso de uso:

```rust
// 1) Em Rust: um Component tipado, com estado próprio.
impl Component for Contador {
    fn name(&self) -> &str { "contador" }
    fn template(&self) -> Template { Template::File("examples/contador/contador.gv".into()) }
    fn init(&mut self, ctx: &mut Context) { ctx.set("contador", self.valor.to_string()); }
    fn update(&mut self, action: &str, _v: Option<&str>, ctx: &mut Context) {
        match action {
            "incrementar" => self.valor += 1,
            "decrementar" => self.valor -= 1,
            _ => return,
        }
        ctx.set("contador", self.valor.to_string());
    }
}
```

```lua
-- 2) No próprio template, num <script> Luau (sem recompilar):
function init()        ctx.contador = ctx.contador or 0 end
function incrementar() ctx.contador = ctx.contador + 1 end
function decrementar() ctx.contador = ctx.contador - 1 end
```

---

## Sumário

- [glacier-ui](#glacier-ui)
  - [Sumário](#sumário)
  - [Por que Glacier](#por-que-glacier)
  - [Instalação](#instalação)
  - [Conceitos e arquitetura](#conceitos-e-arquitetura)
  - [Início rápido](#início-rápido)
    - [Ligando ao `iced`: `GlacierApp::bootstrap`](#ligando-ao-iced-glacierappbootstrap)
  - [Referência de tags](#referência-de-tags)
    - [Layout](#layout)
    - [Conteúdo e controles](#conteúdo-e-controles)
    - [Estruturais (composição, fluxo, recursos)](#estruturais-composição-fluxo-recursos)
  - [Atributos de layout e estilo](#atributos-de-layout-e-estilo)
  - [Data binding](#data-binding)
  - [Controle de fluxo](#controle-de-fluxo)
  - [Inputs de texto](#inputs-de-texto)
  - [Formulários (Reactive Forms)](#formulários-reactive-forms)
  - [Navegação entre telas](#navegação-entre-telas)
  - [Componentes e composição](#componentes-e-composição)
  - [Comportamento em `<script>` Luau](#comportamento-em-script-luau)
    - [`fetch`: rede async via corrotina](#fetch-rede-async-via-corrotina)
    - [`require`: módulos Luau](#require-módulos-luau)
    - [Timers: `after` e `every`](#timers-after-e-every)
    - [`storage`: persistência local](#storage-persistência-local)
    - [`viewport`, `toast`, `confirm`, `navigate`](#viewport-toast-confirm-navigate)
    - [Erros visíveis: `on_error`](#erros-visíveis-on_error)
    - [Streams: SSE e WebSocket](#streams-sse-e-websocket)
  - [Estilos `.gss`](#estilos-gss)
    - [Pseudo-estados: `:hover` / `:focus` / `:active` / `:disabled`](#pseudo-estados-hover--focus--active--disabled)
  - [`<link rel="…">` e temas](#link-rel-e-temas)
  - [Toasts e diálogos (em Rust)](#toasts-e-diálogos-em-rust)
  - [Drag-and-drop: listas reordenáveis](#drag-and-drop-listas-reordenáveis)
  - [Ações built-in](#ações-built-in)
  - [Hot-reload](#hot-reload)
  - [Rede e async em Rust](#rede-e-async-em-rust)
  - [Referência da API](#referência-da-api)
    - [`GlacierUI`](#glacierui)
    - [`EngineMessage`](#enginemessage)
    - [Tipos de apoio (re-exportados na raiz do crate)](#tipos-de-apoio-re-exportados-na-raiz-do-crate)
    - [Globais da camada Luau](#globais-da-camada-luau)
  - [Exemplos](#exemplos)
  - [Publicação no crates.io](#publicação-no-cratesio)
  - [Licença](#licença)

---

## Por que Glacier

- **Declarativo de verdade** — a UI é um arquivo XML, não uma árvore de chamadas Rust.
- **Comportamento onde couber melhor** — em Rust (tipado, com estado forte) ou em Luau dentro do `<script>` (interpretado, sem recompilar).
- **Hot-reload** — edite o XML, os estilos `.gss`, os dados JSON, o tema ou a lógica Luau com a app rodando e veja a mudança na hora; só a lógica em Rust exige recompilar.
- **Data binding por placeholders** — `{chave}` em qualquer atributo, resolvido contra um contexto de estado compartilhado.
- **Componentes** — encapsulam UI + comportamento + estado num único tipo Rust, compostos por `<import>`, referência por nome ou `children()`.
- **Assíncrono sem travar a UI** — `fetch` (HTTP), `sse`/`websocket` (streams) na camada Luau; `ctx.perform` e `Component::subscription` na camada Rust.
- **Estilos reutilizáveis** — classes `.gss` (CSS-like) globais ou com escopo por componente, com a precedência do CSS e pseudo-estados (`:hover`/`:focus`/`:active`/`:disabled`).
- **Renderiza com `iced`** — widgets nativos, multiplataforma, tema configurável.

---

## Instalação

O projeto é um único crate, **`glacier-ui`** — o motor.

```bash
cargo add glacier-ui
```

As dependências vêm junto: `iced 0.14`, `roxmltree`, `image`, `serde_json`,
`regex`, `mlua` (com **Luau** vendorizado — compilado do fonte, sem precisar de
Lua/Luau no sistema), `hyper` + `rustls` para `fetch`, e `tokio-tungstenite` para
WebSocket. O `iced` é re-exportado em `glacier_ui::iced`, então a sua `main`
pode nem listar `iced` como dependência direta. Requer Rust **edition 2024**
(≥ 1.85).

Rode qualquer exemplo do repositório com:

```bash
cargo run --example contador
```

---

## Conceitos e arquitetura

| Peça | Papel |
|---|---|
| **Template XML** | descreve a árvore de UI (layout, texto, botões, imagens, …). |
| **Contexto** | mapa `String -> String` com o estado; templates leem dele via `{chave}`. |
| **`GlacierUI`** | o motor: registra templates/componentes/estilos, avalia o contexto e renderiza para `iced`. |
| **`Component`** | tipo Rust que junta **UI** (template) + **comportamento** (reação a ações) + **estado** próprio. |
| **`<script>` Luau** | comportamento embutido no template, alternativa interpretada ao `Component`. |
| **`EngineMessage`** | mensagens que o `iced` entrega ao motor (cliques, inputs, navegação, reload, efeitos). |
| **Stylesheet `.gss`** | classes de estilo reutilizáveis (CSS-like), globais ou por componente. |

O fluxo de cada frame de estado:

```
XML  ──parse──▶  AST  ──avalia (contexto + estilos + includes + if/for-each)──▶  AST resolvido  ──render──▶  widgets iced
                                                   ▲                                                            │
                                                   └────────── ação vira EngineMessage, roteada ao Component ◀──┘
```

A integração com o `iced` segue o padrão `application(init, update, view)`: o
`update` da app só repassa a mensagem para `motor.dispatch(...)`, e o `view`
chama `motor.render_current()`.

---

## Início rápido

Um app é uma casca fina em volta de um `GlacierUI`: registra os componentes,
repassa mensagens a `dispatch` e renderiza com `render_current`. Toda a lógica
vive nos componentes (Rust) ou nos `<script>` (Luau).

```rust
use glacier_ui::{GlacierUI, EngineMessage, Component, Context, Template};
use iced::{Element, Task, widget::text, Color};

struct Contador { valor: i32 }

impl Component for Contador {
    fn name(&self) -> &str { "contador" }
    fn template(&self) -> Template { Template::File("examples/contador/contador.gv".into()) }
    fn init(&mut self, ctx: &mut Context) { ctx.set("contador", self.valor.to_string()); }
    fn update(&mut self, action: &str, _value: Option<&str>, ctx: &mut Context) {
        match action {
            "incrementar" => self.valor += 1,
            "decrementar" => self.valor -= 1,
            _ => return,
        }
        ctx.set("contador", self.valor.to_string());
    }
}

struct App { motor: GlacierUI }

impl App {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = GlacierUI::new();
        motor.register(Box::new(Contador { valor: 0 })).unwrap();
        motor.set_initial_screen("contador");
        (Self { motor }, Task::none())
    }
    fn update(&mut self, msg: EngineMessage) -> Task<EngineMessage> { self.motor.dispatch(&msg) }
    fn view(&self) -> Element<'_, EngineMessage> {
        self.motor.render_current()
            .unwrap_or_else(|e| text(e).color(Color::from_rgb(1.0, 0.0, 0.0)).into())
    }
    fn subscription(&self) -> iced::Subscription<EngineMessage> {
        GlacierUI::reload_subscription(std::time::Duration::from_millis(500))
    }
}

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .subscription(App::subscription)
        .title("Contador")
        .run()
}
```

Para um comportamento embutido no template, troque `register(Box::new(...))` por
`register_component("contador", "caminho/para/contador.gv")`: se o template tiver
um `<script>`, o motor liga a lógica **Luau** automaticamente (ver
[`examples/contador_macro`](examples/contador_macro)).

### Ligando ao `iced`: `GlacierApp::bootstrap`

Para não repetir `iced::application(App::init, App::update, App::view).subscription(...)`
na mão, implemente o trait `GlacierApp` e chame `App::bootstrap()` — ele pré-liga
os quatro métodos e devolve o builder do `iced` (ainda aceita `.title`, `.theme`,
`.window`, …):

```rust
use glacier_ui::{EngineMessage, GlacierUI, GlacierApp};
use iced::{Element, Subscription, Task};

struct App { motor: GlacierUI }

impl GlacierApp for App {
    type Message = EngineMessage;
    fn init() -> (Self, Task<EngineMessage>) { /* ... */ }
    fn update(&mut self, msg: EngineMessage) -> Task<EngineMessage> { self.motor.dispatch(&msg) }
    fn view(&self) -> Element<'_, EngineMessage> { /* ... */ }
    fn subscription(&self) -> Subscription<EngineMessage> {
        GlacierUI::reload_subscription(std::time::Duration::from_millis(500))
    }
}

fn main() -> iced::Result {
    App::bootstrap().title("Glacier - navegação via script Lua").run()
}
```

Veja [`examples/navegacao_luau`](examples/navegacao_luau).

---

## Referência de tags

Todas as tags aceitam variações de caixa e nomes em inglês **ou** português.

### Layout

| Tag | Aliases | Descrição |
|---|---|---|
| `<Container>` | `container` | caixa única (1 filho lógico); base para cartões/painéis. |
| `<Column>` | `column` | empilha os filhos verticalmente. |
| `<Row>` | `row` | dispõe os filhos horizontalmente. |
| `<Scrollable>` | `Scroll`, `Rolagem` | viewport rolável de 1 filho; `direction`: `vertical` (padrão), `horizontal`, `both`. |
| `<Rule>` | `Divider`, `Divisoria` | linha separadora; `direction`: `horizontal` (padrão) ou `vertical`. |

### Conteúdo e controles

| Tag | Aliases | Atributos próprios |
|---|---|---|
| `<Text>` | `text` | `content`/`texto`, `size`/`tamanho`, `bold`/`negrito`, `color`/`cor`, `textAlign` |
| `<Button>` | `button`, `Botao` | `text`/`texto`, `on_click`/`aoClicar`, `navigateTo`/`irPara`, `navigateBack`/`voltar`, `color`/`cor` |
| `<TextInput>` | `Input`, `EntradaTexto` | `placeholder`/`dica`, `value`/`valor`, `onChange`/`aoMudar`, `secure`/`password` (mascara), `formControl` (liga a um `FormControl`) |
| `<TextArea>` | `TextEditor`, `Editor`, `AreaTexto` | `placeholder`/`dica`, `value`/`valor`, `onChange`/`aoMudar` (editor multilinha) |
| `<Form>` | `Formulario` | `onSubmit`/`aoSubmeter`, `name`/`nome` — renderiza como `<Column>` |
| `<Select>` | `Dropdown`, `PickList`, `ComboBox`, `Seletor` | `options`/`items` (chave com array JSON), `value`/`valor`, `onChange`/`onSelect`, `placeholder`, `labelField` (padrão `label`), `valueField` (padrão `value`) |
| `<Image>` | `Imagem` | `source`/`src`/`caminho`, `clip="Circle"` (corte circular) |
| `<Svg>` | `Icon`, `Icone` | `source`/`src`, `color`/`cor` (tinge o ícone vetorial) |
| `<Checkbox>` | `Check` | `label`, `checked`/`value` (chave de contexto), `onToggle`/`onChange` |
| `<Toggle>` | `Toggler`, `Switch` | `label`, `checked`/`value`, `onToggle`/`onChange` |

### Estruturais (composição, fluxo, recursos)

| Tag | Aliases | Descrição |
|---|---|---|
| `<import>` | `Import`, `Importar` | declara um componente carregado de um arquivo: `name`/`nome`/`as`, `from`/`de`/`src`. |
| `<Include>` | `Incluir` | inclui outro template: `src`/`fonte`; demais atributos viram props. |
| `<NomeDoComponente .../>` | — | qualquer tag desconhecida referencia um componente por nome; atributos viram props. |
| `<ForEach>` | `For` | repete os filhos por item: `items`/`itens`, `var`/`variavel`. |
| `<if>` | `Se` | renderiza condicionalmente: `cond`, `equals`, `notEquals`. |
| `<else>` | `Senao` | renderiza quando o `<if>` imediatamente anterior foi falso. |
| `<link>` | `Link` | carrega um recurso: stylesheet, componente, dados ou tema. |
| `<style>` | `Style` | classes `.gss` inline (global por padrão ou `scoped="true"`), ou externa com `href`. |
| `<script>` | — | comportamento Luau embutido (inline ou `src="arquivo.luau"`). |

---

## Atributos de layout e estilo

Disponíveis em **qualquer** tag:

| Atributo | Aliases | Valores |
|---|---|---|
| `width` | `largura`, `w` | `fill`, `shrink` ou número (px) |
| `height` | `altura`, `h` | `fill`, `shrink` ou número (px) |
| `padding` | `espacamento_interno` | `"10"`, `"10 20"` (vert. horiz.) ou `"10 20 30 40"` (top right bottom left) |
| `alignX` | `align_x`, `align` | `start`, `center`, `end` |
| `alignY` | `align_y` | `start`, `center`, `end` |
| `spacing` | `espacamento` | número (espaço entre filhos de `Row`/`Column`) |
| `background` | `bg`, `fundo` | cor hex |
| `gradient` | `gradiente` | `"#a #b"` (cima→baixo) ou `"<ângulo> #a #b [#c …]"`; vence `background` |
| `borderRadius` | `border_radius`, `raio_borda` | número |
| `borderWidth` | `border_width` | número |
| `borderColor` | `border_color` | cor hex |
| `class` | `classe` | classes `.gss` separadas por espaço |
| `font` | `fonte`, `font-family` | `mono`/`monospace`/`code` ou `bold` — em `Text`/`Button` |
| `onPress` | `aoPressionar` | ação no **pressionar** (envolve em `mouse_area`); viabiliza `onPress="window:drag"` |
| `onDoubleClick` | `aoClicarDuplo` | ação no **duplo-clique** (ex.: `window:maximize` na barra de título) |
| `cursor` | `cursorIcon` | `pointer`, `text`, `grab`, `grabbing`, `move`, `crosshair`, `wait`, `not-allowed`, `resize-h/v/ne/nw`, … |
| `hidden` | `oculto` | `true`/`false` — remove do layout (não ocupa espaço) |
| `disabled` | `desabilitado` | `true`/`false` — desativa a interação de `Button`/`TextInput`/`Checkbox`/`Toggle` |

- **Eixos:** o eixo cruzado de uma `Column` é o `alignX`; o de uma `Row` é o `alignY`.
- **Cores:** hex `#RRGGBB` ou `#RRGGBBAA`.
- **`fill` só "enche"** se todo container pai até ele também for `width=fill` (o default da maioria dos widgets é `shrink`).

---

## Data binding

Qualquer valor de atributo pode conter placeholders `{chave}`, substituídos pelos
valores do contexto durante a avaliação:

```xml
<Text content="Olá, {user_name}!" color="{cor_texto}" />
<Container background="{painel_bg}"> ... </Container>
```

O componente publica valores com `ctx.set("user_name", "Clara")` (Rust) ou
`ctx.user_name = "Clara"` (Luau) — ou `motor.define_data("user_name", "Clara")`
por fora. Sempre que o contexto muda, o motor reavalia os templates e a UI
reflete o novo valor. **Chaves ausentes viram string vazia.** O estado é
compartilhado entre todas as telas.

---

## Controle de fluxo

A forma recomendada são **atributos diretiva** aplicados em qualquer elemento
(estilo Vue/Angular). A sintaxe antiga de tags-invólucro (`<if>`, `<else>`,
`<ForEach>`) continua suportada por retrocompatibilidade.

**Condicional** — `if` renderiza truthy (`true`/`1`/`yes`/`on`/`sim`); `else`
(pelado) se conecta ao `if` anterior; `equals`/`notEquals` comparam explicitamente:

```xml
<Column if="{logado}"><Text content="Bem-vindo!" /></Column>
<Column else><Text content="Por favor, conecte-se." /></Column>

<Text content="Painel Admin"  if="{papel}" equals="admin" />
<Text content="Acesso Comum"  if="{papel}" notEquals="admin" />
```

> *XML estrito:* atributos pelados como `else` não são válidos no padrão; o
> Glacier faz um pré-processamento transparente convertendo `else` → `else=""`.

**Loop** — `for-each` itera sobre um **array JSON** do contexto; `var` nomeia a
variável (padrão `item`). Objetos viram `{u.campo}`; escalares ficam em `{u}`:

```xml
<CartaoUsuario for-each="usuarios" var="u"
    nome="{u.nome}" cargo="{u.cargo}" cor="{u.cor}" />
```

```rust
ctx.set("usuarios", serde_json::json!([
    { "nome": "Clara",  "cargo": "Engenheira", "cor": "#89B4FA" },
    { "nome": "Sophia", "cargo": "Designer",   "cor": "#F5C2E7" },
]).to_string());
```

Combinados no mesmo elemento, `for-each` tem precedência: desenrola o loop
primeiro e o `if` filtra cada item gerado no contexto local. Veja
[`examples/condicional`](examples/condicional) e [`examples/lista`](examples/lista).

---

## Inputs de texto

`<TextInput>` faz binding bidirecional: `value` aponta para a chave exibida e
`onChange` dispara uma ação com o novo texto a cada tecla. No `update`, o texto
chega em `value: Option<&str>`:

```xml
<TextInput placeholder="Seu nome..." value="user_name" onChange="mudar_nome" width="fill" padding="10" />
```

```rust
fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context) {
    if action == "mudar_nome" {
        if let Some(v) = value { ctx.set("user_name", v); }
    }
}
```

Veja [`examples/perfil`](examples/perfil) e [`examples/navegacao`](examples/navegacao).

---

## Formulários (Reactive Forms)

Inspirado no Angular Reactive Forms: `FormBuilder` declara os `FormControl`s
(nome, valor inicial, validadores) do lado Rust; o componente guarda o `Form` no
seu estado; e o template liga cada input a um controle pelo atributo `formControl`
— o motor cuida do resto (o texto vai para o controle certo, Enter submete e
avança para o próximo campo).

```rust
let form = FormBuilder::new("login")
    .control(FormControl::new("username", "").required().min_length(3))
    .control(FormControl::new("password", "").required().min_length(6))
    // A lógica de submissão fica junto dos controles.
    .on_submit(|form, ctx| {
        if form.is_valid() {
            ctx.set("status", format!("Bem-vindo, {}!", form.value("username")));
        } else {
            form.validate();                 // marca também campos não tocados
            form.errors_to_context(ctx, "erro_");
        }
    })
    .build();
```

```xml
<Form onSubmit="entrar" name="login" width="fill">
    <TextInput formControl="username" placeholder="usuário" width="fill" />
    <TextInput formControl="password" placeholder="senha" secure="true" width="fill" />
    <Button text="Entrar" on_click="entrar" />
</Form>
```

`TextInput formControl="username"` sem `value`/`onChange` usa o nome do controle
para os dois. No `update`, `Form::has_control` reconhece a ação de campo sem um
`match` por campo — e a **submissão** vai por um método próprio, `on_form_submit`,
então atualização de campo e submissão nunca competem pelo mesmo `match`:

```rust
fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context) {
    if self.form.has_control(action) {
        self.form.set_value(action, value.unwrap_or_default());
        self.form.sync_to_context(ctx);
    }
}
fn on_form_submit(&mut self, _action: &str, ctx: &mut Context) {
    self.form.submit(ctx);   // roda a closure registrada em .on_submit(...)
}
```

Validadores: `.required()`, `.min_length(n)`, `.max_length(n)`,
`.pattern(regex)`, `.validator(|v| Ok(()) | Err(msg))`. `Form::errors_to_context`
publica o primeiro erro de cada campo (`"{prefixo}{nome}"`) para exibir inline com
`Text "{erro_username}"`. Enter em qualquer campo dispara o `onSubmit` **e** avança
o foco — dá para preencher e enviar o formulário sem tocar no mouse. Veja
[`examples/formulario_login`](examples/formulario_login).

---

## Navegação entre telas

Cada tela é um componente registrado. Há três formas de trocar de tela:

**1. Declarativa (atributos no XML):**

```xml
<Button text="Ver perfil" navigateTo="perfil" />
<Button text="Voltar" navigateBack="true" />
```

**2. Imperativa (Rust):** de dentro do `update`, via `ctx.navigate_to(...)` /
`ctx.navigate_back()`; ou no motor, `motor.navigate_to(...)`.

**3. Decidida por script (Luau):** `navigate(tela)` / `navigate_back()` — o
script decide se navega (ex.: só depois de validar o login):

```lua
function entrar()
    if ctx.usuario == "admin" and ctx.senha == "123" then
        navigate("dashboard_luau")
    else
        ctx.erro = "Usuário ou senha inválidos."
    end
end
```

O motor mantém uma **pilha de histórico**: `navigateTo` empilha a tela atual;
`navigateBack` volta. O estado de contexto é compartilhado entre telas. Veja
[`examples/navegacao`](examples/navegacao) (declarativa) e
[`examples/navegacao_luau`](examples/navegacao_luau) (via script).

---

## Componentes e composição

**Composição a nível de template** — duas formas equivalentes; os atributos viram
**props** interpoladas no contexto local do filho:

```xml
<import name="PerfilCard" from="examples/perfil/perfil_card.gv" />
<PerfilCard nome="{user_name}" cargo="{user_role}" />

<!-- ou -->
<Include src="perfil_card.gv" nome="{user_name}" />
```

**O trait `Component`** — encapsula UI + comportamento + estado:

```rust
pub trait Component {
    fn name(&self) -> &str;                  // nome (registro + roteamento)
    fn template(&self) -> Template;          // Template::File(path) | Template::Inline(xml)
    fn init(&mut self, ctx: &mut Context) {} // estado inicial (opcional)
    fn children(&self) -> Vec<Box<dyn Component>> { Vec::new() } // sub-componentes (opcional)
    fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context);
    fn on_form_submit(&mut self, action: &str, ctx: &mut Context) {} // onSubmit de um <Form>
    fn subscription(&self) -> iced::Subscription<EngineMessage> { /* streams/rede */ }
}
```

**Componentes aninhados e roteamento** — um `Component` pode **possuir** outros
via `children()`. Ao registrar o pai, o motor registra os filhos em cascata, e as
ações que saem da UI de um filho são roteadas para o `update` **do filho**. O motor
prefixa as ações da subárvore com o nome do componente
(`incrementar` → `CartaoContador::incrementar`); no `dispatch`:

- prefixo de um componente com comportamento → vai para ele;
- ação sem prefixo, ou prefixo só de UI → cai na **tela ativa** (fallback) — é o
  que mantém includes puramente visuais funcionando.

> **Limite conhecido:** um filho referenciado N vezes (ex.: dentro de `<ForEach>`)
> compartilha um único objeto e um único `update`.

**`ContextVar`** — açúcar para declarar variáveis legíveis em vez de chaves soltas:

```rust
ctx.set_var(&ContextVar::new("user_name", "Clara Silva"));
```

Veja [`examples/aninhado`](examples/aninhado), [`examples/lista`](examples/lista)
e [`examples/perfil`](examples/perfil).

---

## Comportamento em `<script>` Luau

Dá para colocar o comportamento **dentro do próprio template**, num bloco
`<script>` com funções **Luau**. Ao registrar com `register_component`, o motor
detecta o `<script>`, carrega o script e roteia cada ação
(`on_click`/`onChange`/`onSubmit`) para a função de mesmo nome — tudo
**interpretado em tempo de execução**, sem recompilar.

```xml
<Button text="+" on_click="incrementar" />
<Button text="-" on_click="decrementar" />

<script>
function init()        ctx.contador = ctx.contador or 0 end
function incrementar() ctx.contador = ctx.contador + 1 end
function decrementar() ctx.contador = ctx.contador - 1 end
</script>
```

```rust
motor.register_component("contador", "examples/contador_macro/contador_macro.gv")?;
```

Como funciona:

- cada função Luau vira uma ação homônima (casada com `on_click`/`onChange`/`onSubmit`);
- o **contexto** do motor é a tabela global `ctx`: ler `ctx.contador` devolve o valor atual, atribuir grava de volta. Luau coage strings numéricas, então `ctx.contador + 1` sobre `"0"` volta `"1"`. Atribuir `ctx.x = nil` **remove** a chave;
- atribuir uma **tabela** a `ctx.x` serializa via `json.encode` automaticamente;
- ações de `onChange` recebem o texto digitado como 1º argumento **e** na global `value`;
- `init()` (opcional) semeia o estado inicial.

**Arquivo externo** — aponte para um `.luau` separado com `src` (resolvido
relativo ao diretório do template):

```xml
<script src="contador_externo.luau"></script>
```

Veja [`examples/contador_macro`](examples/contador_macro) (inline) e
[`examples/contador_externo`](examples/contador_externo) (externo).

### `fetch`: rede async via corrotina

`fetch(url, opts)` faz HTTP/HTTPS (via `hyper` + `rustls`). Ela **suspende a
corrotina** da ação e retoma quando a resposta chega — a UI **não trava**, mas o
código fica com cara de `await`, linear:

```lua
function buscar()
    ctx.status = "carregando..."                     -- já aparece na tela
    local res = fetch("https://api.ipify.org?format=json") -- suspende aqui
    if res.ok then
        ctx.resultado = res.body                     -- retomou com a resposta
        ctx.status = "ok (" .. res.status .. ")"
    else
        ctx.status = "falhou"
    end
end
```

O retorno é `{ ok, status, body, error }`. O 2º argumento `opts` é opcional:
`{ method = "POST", body = "...", headers = { ["Authorization"] = "..." } }`.
Veja [`examples/fetch_luau`](examples/fetch_luau).

### `require`: módulos Luau

Extraia lógica em **bibliotecas** `.luau` e importe com `require` — encapsuladas e
reutilizáveis entre componentes:

```lua
-- net/http_client.luau
local Client = {}
Client.__index = Client
function Client.new(base) return setmetatable({ base = base, headers = {} }, Client) end
function Client:get(path) return fetch(self.base .. path, { headers = self.headers }) end
return Client
```

```lua
local http = require("net/http_client")
local api  = http.new("https://api.exemplo")
function carregar() local res = api:get("/dados"); if res.ok then ctx.dados = res.body end end
```

`require("a/b")` procura `a/b.luau` (e `a/b/init.luau`) **nesta ordem**: (1) o
diretório do template; (2) um subdir `lib/`; (3) cada caminho em
`GLACIER_LUA_PATH` (separados por `:`). O módulo roda no **mesmo** interpretador,
então enxerga `fetch` e as globais; é carregado **uma vez** e cacheado. Veja
[`examples/imports_luau`](examples/imports_luau).

### Timers: `after` e `every`

- **`after(ms, fn)`** — temporizador de **disparo único** (setTimeout). Não suspende: agenda `fn` (função ou nome de global) e devolve um handle cancelável.
- **`every(ms, fn)`** — temporizador **repetitivo** (setInterval), construído sobre `after`: reagenda a si mesmo a cada disparo, com `:cancel()` estável entre repetições.

```lua
-- dispara uma vez após 3s, cancelável antes disso
local t = after(3000, "tempo_esgotado")
t:cancel()

-- repete a cada 1s até cancelar
cronometro = every(1000, "tique")
function tique() ctx.tiques = ctx.tiques + 1 end
function parar() cronometro:cancel() end
```

Veja [`examples/robustez_luau`](examples/robustez_luau).

### `storage`: persistência local

`storage.get/set/remove` guardam JSON em disco por componente, sobrevivendo a
reiniciar o processo:

```lua
function init()   ctx.rascunho = storage.get("rascunho") or "" end
function salvar() storage.set("rascunho", ctx.rascunho) end
```

### `viewport`, `toast`, `confirm`, `navigate`

- **`viewport()`** → `{ width, height }` em px lógicos (tamanho atual da janela).
- **`toast(opts)`** — notificação efêmera; `opts` = string ou `{ message, kind?, title? }` (kind: `info`/`success`/`warning`/`error`).
- **`confirm(opts)`** — diálogo modal; o botão de confirmação despacha `opts.confirm_action`.
- **`navigate(tela)` / `navigate_back()`** — navegação (ver [Navegação](#navegação-entre-telas)).

Nenhuma delas suspende a corrotina — o motor aplica o efeito e retoma na hora.

### Erros visíveis: `on_error`

Um erro de runtime no script vira **visível** em vez de sumir num `eprintln!`.
Defina `on_error(msg)` opcional para controlar a mensagem ao usuário (e guardar o
erro técnico); sem ele, o motor promove a mensagem crua a um **toast** automático:

```lua
function on_error(msg)
    ctx.ultimo_erro = msg
    toast({ title = "Erro no script", message = "Algo deu errado.", kind = "error" })
end
```

### Streams: SSE e WebSocket

Ao contrário do `fetch` (one-shot), `sse` e `websocket` são streams de **vida
longa**: NÃO suspendem — registram o stream e devolvem um handle na hora. Cada
evento recebido chama o handler nomeado em `opts` (`on_open`, `on_message`,
`on_error`, `on_close`), que escreve em `ctx` como qualquer ação:

```lua
sse_conn = sse("https://sse.dev/test", {
    on_open = "sse_aberto", on_message = "sse_recebeu", on_close = "sse_fechou",
})
function sse_recebeu(data) ctx.sse_msg = data end
function fechar() sse_conn:close() end

ws_conn = websocket("wss://echo.websocket.org", { on_message = "ws_recebeu" })
ws_conn:send("ping")   -- envia pela conexão viva
```

**Importante:** os streams viram `iced::Subscription`s produzidas por
`GlacierUI::subscription`. O `subscription()` do app precisa incluir
`self.motor.subscription()` — sem isso, nenhuma conexão é aberta. Veja
[`examples/stream_lua`](examples/stream_lua).

---

## Estilos `.gss`

Um `.gss` (*glacier stylesheet*) é um arquivo CSS-like que tira estilos repetidos
da markup e os agrupa em **classes**. Aplique com `class="..."`:

```gss
/* Comentários // de linha e de bloco. '#' nunca é comentário (cores ficam intactas). */
.card  { padding: 24; background: #1E1E2E; border-radius: 16; border-width: 1; border-color: #313244; align-x: Center; }
.title { size: 26; bold: true; color: #CDD6F4; }
```

```xml
<Container class="card"><Text class="title" content="Olá" /></Container>
```

**Precedência (igual à do CSS):**

1. um **atributo inline** no nó sempre vence tudo;
2. um seletor de **id** (`#nome`) vence a classe (especificidade maior);
3. classes aplicam da **esquerda para a direita** (`class="a b"` → `b` sobrepõe `a`);
4. estilos **globais** primeiro, depois os **com escopo** do componente.

**Seletor de id.** Além de `.classe`, um bloco `#nome { }` casa o atributo
`id="nome"` do nó e é aplicado **por cima** das classes (mas ainda por baixo do
inline). Bom para estilizar *um* elemento sem inventar uma classe descartável.
Aceita pseudo-estados (`#salvar:hover { }`) e vale dentro de `@media`. A
unicidade não é exigida — vários nós podem compartilhar o mesmo `id`.

```gss
.title  { color: #CDD6F4; }
#hero   { color: #F38BA8; }   /* vence a .title onde id="hero" */
```

```xml
<Text id="hero" class="title" content="Olá" />
```

**Propriedades reconhecidas:** `width`/`w`, `height`/`h`, `padding`, `spacing`,
`align-x`/`align-y`, `background`/`bg`, `border-radius`, `border-width`,
`border-color`, `color`, `text-color`, `size`, `bold`, `hidden`.

Carregue por código (`motor.load_stylesheet("styles/app.gss")` — sempre **global**)
ou declare no template. Um `<style>` inline é **global** por padrão, ou **com
escopo** ao componente com `scoped="true"` — a única forma de escopar um `.gss`:

```xml
<style>
    .card  { padding: 24; background: #1E1E2E; border-radius: 16; }
</style>
<style scoped="true">
    .only_here { color: red; }
</style>
```

Veja [`examples/estilos`](examples/estilos) (arquivo + `<link>`) e
[`examples/estilos_inline`](examples/estilos_inline) (bloco `<style>`).

### Pseudo-estados: `:hover` / `:focus` / `:active` / `:disabled`

Uma classe pode declarar overlays por pseudo-estado — cada bloco sobrescreve só os
campos que declara, por cima da regra base (igual ao CSS):

```gss
.btn          { background: #313244; text-color: #CDD6F4; border-radius: 8; }
.btn:hover    { background: #45475A; }
.btn:active   { background: #1E1E2E; }
.btn:disabled { background: #181825; text-color: #6C7086; }
```

Cada pseudo-estado é mapeado para o `Status` nativo do widget do iced — nada de
rastrear hover manualmente. **Cobertura atual:**

- **`Button`** — `:hover`/`:active`/`:disabled` completos (requer uma `color` base na classe).
- **`TextInput`** — `:hover`/`:focus`/`:disabled` completos.
- **`Select`** — só `:hover` (o `pick_list` do iced não tem `Status::Disabled`).
- **`Checkbox`/`Toggle`** — só o atributo `disabled` (usam o visual padrão do tema).

Veja [`examples/pseudo_estados`](examples/pseudo_estados).

---

## `<link rel="…">` e temas

O `<link>` declara um recurso externo; `rel` escolhe o tipo:

| `rel` | O que faz | Atributos |
|---|---|---|
| `stylesheet` (padrão) | carrega um `.gss` **global** | `href` |
| `import` / `component` | carrega outro template (igual a `<import>`) | `href`, `as`/`name` |
| `data` | faz merge de um JSON no contexto | `href`, `as`/`name` |
| `theme` | aplica uma paleta como `iced::Theme` | `href` |

```xml
<link rel="stylesheet" href="styles/estilos.gss" />
<link rel="import" href="templates/perfil_card.gv" as="PerfilCard" />
<link rel="data" href="data/equipe.json" as="app" />   <!-- {app.campo}, <ForEach items="app.lista"> -->
<link rel="theme" href="styles/theme.json" />
```

**Tema** — um JSON de cores hex aplicado como `iced::Theme`:

```json
{ "name": "Mocha", "background": "#181825", "text": "#CDD6F4",
  "primary": "#89B4FA", "success": "#A6E3A1", "danger": "#F38BA8" }
```

Ligue-o na `application` via `motor.theme()` (devolve `Theme::Dark` se nenhum foi
carregado) — também resolve o "fundo branco" padrão do `iced`:

```rust
iced::application(App::new, App::update, App::view)
    .theme(|app| app.motor.theme())
    .run()
```

Como os `<import>`, os `<link>`/`<style>`/`<script>` podem ficar no topo do
arquivo, como irmãos da raiz, e não renderizam nada.

---

## Toasts e diálogos (em Rust)

**Toasts** — notificações efêmeras empilhadas no canto, dispensadas sozinhas após
alguns segundos (ou pelo "×"):

```rust
ctx.show_toast(ToastSpec::success("Serviço publicado."));
ctx.show_toast(ToastSpec::warning("Fica 10s.").with_title("Custom").with_duration(Duration::from_secs(10)));
```

Requer `GlacierUI::toast_subscription(...)` no `subscription()` do app — sem ele,
os toasts só fecham no "×". Veja [`examples/toasts`](examples/toasts).

**Diálogos** — modais estilo `QMessageBox` (informação, aviso, erro, pergunta,
confirmação), sobrepostos pelo motor:

```rust
ctx.show_dialog(DialogSpec::error("Falha no deploy", "Porta 8080 já em uso."));
ctx.show_dialog(
    DialogSpec::confirm("Excluir projeto", "Essa ação não pode ser desfeita.")
        .with_detail("3 serviços serão removidos.")
        .with_button(DialogButton::discard("excluir_confirmado")),
);
```

Os botões despacham ações (`"ok"`, `"yes"`, `"no"`, `"cancel"`, ou a ação
customizada) roteadas ao `update` — o motor já fechou o diálogo antes. Veja
[`examples/dialogs`](examples/dialogs). (Da camada Luau, use `confirm(opts)`.)

---

## Drag-and-drop: listas reordenáveis

Um `<ForEach>` com `onReorder` + `reorderKey` vira uma lista reordenável por
arrasto: arraste pelo elemento marcado `dragHandle="true"`. Ao soltar, `onReorder`
entrega a nova ordem (array JSON dos valores de `reorderKey`). Durante o arrasto,
o item agarrado recebe `{t.__dragging} = "true"` para destacá-lo:

```xml
<ForEach items="tarefas" var="t" onReorder="reordenar" reorderKey="id">
    <Row if="{t.__dragging}" equals="true" background="#434C5E" borderColor="#88C0D0" ...>
        <Text content="⋮⋮" dragHandle="true" cursor="grabbing" />
        <Text content="{t.nome}" width="fill" />
    </Row>
    <Row else="true" background="#3B4252" ...>
        <Text content="⋮⋮" dragHandle="true" cursor="grab" />
        <Text content="{t.nome}" width="fill" />
    </Row>
</ForEach>
```

Requer `self.motor.subscription()` no `subscription()` do app (carrega o listener
global de "soltar o mouse" que encerra o drag). Veja
[`examples/lista_reordenavel`](examples/lista_reordenavel).

---

## Ações built-in

Algumas ações de `on_click`/`onPress` são tratadas pelo motor, sem código no
componente:

| Ação | Efeito |
|---|---|
| `clipboard:<chave>` | copia o valor de contexto `<chave>` para a área de transferência |
| `window:minimize` | minimiza a janela |
| `window:maximize` | alterna maximizar/restaurar (alias `window:toggle_maximize`) |
| `window:close` | fecha a janela |
| `window:drag` | inicia o arraste — use em `onPress` de uma região da barra de título |
| `window:resize:<dir>` | inicia o redimensionamento — `<dir>` ∈ `n,s,e,w,ne,nw,se,sw` |

Permitem montar uma barra de título customizada para uma janela sem decorações
(`decorations: false` nas `window::Settings`):

```xml
<Row width="fill" onPress="window:drag"><Text content="Meu App" /></Row>
<Button text="—" on_click="window:minimize" />
<Button text="✕" on_click="window:close" />
```

---

## Hot-reload

Recursos carregados de arquivo são recarregados quando mudam em disco: **templates**
(inclusive `<import>`/`<link>` novos), **stylesheets `.gss`**, **dados**
(`<link rel="data">`), **tema** e a **lógica Luau** de um `<script src>`. Ligue a
subscription:

```rust
fn subscription(&self) -> iced::Subscription<EngineMessage> {
    GlacierUI::reload_subscription(std::time::Duration::from_millis(500))
}
```

Edite o XML, o `.gss`, o JSON, o tema ou o `.luau` e veja a UI atualizar sem
recompilar. Só a lógica em Rust exige um novo build.

---

## Rede e async em Rust

Além da camada Luau, um `Component` pode disparar I/O por **efeitos** e receber
fluxos por **subscriptions**.

**Efeitos pontuais** — dentro do `update`, `ctx.perform(future)`. Ao completar,
os pares `(chave, valor)` são mesclados no contexto e a UI reavalia. O
`EffectOutcome` também carrega um **toast** opcional:

```rust
fn update(&mut self, action: &str, _v: Option<&str>, ctx: &mut Context) {
    if action == "salvar" {
        ctx.perform(async {
            match salvar_no_servidor().await {
                Ok(_)  => EffectOutcome::data(vec![("salvo".into(), "true".into())])
                    .with_toast(ToastSpec::success("Salvo.")),
                Err(e) => EffectOutcome::toast(ToastSpec::error(format!("Falha: {e}"))),
            }
        });
    }
}
```

Para isso, `dispatch` devolve `iced::Task<EngineMessage>` — repasse-a no `update`
da app (`self.motor.dispatch(&msg)`).

**Fluxos contínuos** — implemente `Component::subscription` devolvendo uma
`iced::Subscription` que emita `EngineMessage::ContextPatch(pares)`. O motor agrega
tudo em `GlacierUI::subscription`, que você liga à app. Cada item recebido mescla
no contexto e reavalia — sem escrever `match` de mensagens.

```rust
fn subscription(&self) -> iced::Subscription<EngineMessage> {
    Subscription::batch([
        self.motor.subscription(),                                    // rede/streams dos componentes
        GlacierUI::reload_subscription(Duration::from_millis(500)),   // hot-reload
    ])
}
```

---

## Referência da API

### `GlacierUI`

| Método | Descrição |
|---|---|
| `new()` | cria um motor vazio. |
| `register(Box<dyn Component>)` | registra um componente (UI + comportamento + `children()` em cascata). |
| `register_component(name, path)` | registra de um arquivo; liga o comportamento Luau se houver `<script>`. |
| `load_stylesheet(path)` | carrega/recarrega um `.gss` **global** e reavalia tudo. |
| `theme()` | o `iced::Theme` do `<link rel="theme">`, ou `Theme::Dark`. |
| `dispatch(&EngineMessage)` | roteia a mensagem, aplica navegação/reload/patch e devolve uma `iced::Task` com os efeitos. |
| `subscription()` | agrega as `Component::subscription` (rede, streams, drag) numa `iced::Subscription`. |
| `set_initial_screen(name)` | define a tela ativa inicial e limpa o histórico. |
| `navigate_to(name)` / `navigate_back()` | navegação imperativa. |
| `render_current()` / `render(name)` | renderiza a tela ativa / um componente. |
| `define_data(k, v)` / `get_data(k)` | manipulam o contexto por fora. |
| `reevaluate_all()` / `check_reload()` | reavalia tudo / recarrega arquivos alterados. |
| `reload_subscription(period)` / `toast_subscription(period)` | subscriptions de hot-reload / expiração de toasts. |

### `EngineMessage`

```rust
pub enum EngineMessage {
    UiClick(String),                                   // on_click
    UiInputChanged { action: String, value: String },  // onChange
    Navigate(String),                                  // navigateTo
    NavigateBack,                                      // navigateBack
    FileChanged(String),                               // tick do hot-reload
    ContextPatch(Vec<(String, String)>),               // subscriptions -> contexto
    EffectOutcome(EffectOutcome),                      // efeito async: patch + toast
    UiSubmit { action: String, next_focus: Option<String> }, // Enter num formControl
    // ... DragStart/DragHover/DragEnd (drag-and-drop), UiEditorAction (TextArea),
    //     LuauStream / LuauTimer (streams e timers da camada Luau)
}
```

### Tipos de apoio (re-exportados na raiz do crate)

- `GlacierApp` — trait com `bootstrap()` (atalho para `iced::application`).
- `Template::File(String)` | `Template::Inline(String)`.
- `Context` — `get`, `set`, `set_var`, `navigate_to`, `navigate_back`, `perform`, `show_toast`, `show_dialog`, `close_dialog`.
- `EffectOutcome` — `::data(...)` / `::toast(...)` / `.with_toast(...)`.
- `ContextVar::new(key, value)` · `Nav::To(String)` | `Nav::Back`.
- `FormBuilder` / `Form` / `FormControl` / `Validator` (ver [Formulários](#formulários-reactive-forms)).
- `DialogSpec` / `DialogButton` / `DialogIcon` / `ButtonRole` (ver [Diálogos](#toasts-e-diálogos-em-rust)).
- `ToastSpec` / `ToastKind`.
- `iced` re-exportado como `glacier_ui::iced` (e `Element`, `Task`, `Subscription`, `Font`, `Point`, `Size`, `window`).

### Globais da camada Luau

| Global | Assinatura | Suspende? |
|---|---|---|
| `ctx` | tabela = contexto do motor (ler/escrever `{chave}`) | — |
| `value` | texto do `onChange` (1º arg das ações de input) | — |
| `fetch(url, opts?)` | HTTP → `{ ok, status, body, error }` | **sim** |
| `sse(url, opts)` | abre SSE, devolve handle `{ :close() }` | não |
| `websocket(url, opts)` | abre WS, devolve handle `{ :send(t), :close() }` | não |
| `after(ms, fn)` | timer único, devolve handle `{ :cancel() }` | não |
| `every(ms, fn)` | timer repetitivo, devolve handle `{ :cancel() }` | não |
| `viewport()` | `{ width, height }` | não |
| `toast(opts)` | notificação efêmera | não |
| `confirm(opts)` | diálogo modal | não |
| `navigate(tela)` / `navigate_back()` | navegação | não |
| `storage.get/set/remove` | persistência local em JSON | não |
| `json.encode/decode/array` | (de)serialização JSON | não |
| `require(mod)` | importa uma biblioteca `.luau` | não |
| `on_error(msg)` | hook opcional de erro de script | — |

---

## Exemplos

Todos em [`examples/`](examples), rodáveis com `cargo run --example <nome>`.

| Exemplo | Demonstra |
|---|---|
| `contador` | `Component` básico com estado e cliques (Rust). |
| `contador_macro` | comportamento embutido via `<script>` Luau + `<style>` inline. |
| `contador_externo` | `<script src="...luau">` externo; `onChange` num input define o passo. |
| `perfil` | inputs, `<import>` de um cartão, `Image` circular e `ContextVar`. |
| `lista` | `<ForEach>` sobre JSON com um componente (`<import>`) por item. |
| `lista_reordenavel` | drag-and-drop: `onReorder`/`reorderKey`/`dragHandle`. |
| `condicional` | `<if>`/`<else>` (truthy e comparação). |
| `aninhado` | componente dentro de outro via `children()`, roteamento por namespace. |
| `navegacao` | múltiplas telas, histórico e `navigateTo`/`navigateBack` declarativos. |
| `navegacao_luau` | navegação decidida pelo script (`navigate` após validar); `GlacierApp::bootstrap`. |
| `formulario_login` | `Form`/`FormBuilder`/`FormControl`: validação, Enter para submeter/avançar. |
| `estilos` | `.gss` de arquivo (global + escopado via `<link>`), classes e tema. |
| `estilos_inline` | classes `.gss` inline e escopadas via bloco `<style>`. |
| `pseudo_estados` | `:hover`/`:focus`/`:active`/`:disabled` em Button/TextInput/Select. |
| `dialogs` | diálogos modais estilo QMessageBox (Rust). |
| `toasts` | toasts info/sucesso/aviso/erro, com título e duração customizados. |
| `fetch_luau` | chamada HTTP (`fetch`) do Luau, async via corrotina. |
| `imports_luau` | `require` de bibliotecas Luau (client de rede + utilitários). |
| `robustez_luau` | timers (`after`/`every`), `storage`, `viewport`, tabelas em `ctx`, `on_error`. |
| `stream_lua` | streams de vida longa: SSE + WebSocket a partir do Luau. |

---

## Publicação no crates.io

```bash
cargo login             # token de https://crates.io/settings/tokens
cargo publish --dry-run # valida o empacotamento
cargo publish           # publica glacier-ui
```

---

## Licença

Licenciado sob **MIT OR Apache-2.0**, à sua escolha.
