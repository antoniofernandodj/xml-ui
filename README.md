# glacier-ui

**Glacier** é um motor de UI declarativa para Rust: você descreve a interface em
**XML** e o motor a renderiza com [`iced`](https://iced.rs). Tudo com
**hot-reload**, **data binding**, **componentes** reutilizáveis, **navegação**,
**stylesheets `.gss`** (CSS-like) e **comportamento** encapsulado em tipos Rust
(ou embutido no próprio XML via `<script>`).

```xml
<Container padding="20" alignX="Center" alignY="Center" width="fill" height="fill" background="#2E3440">
    <Column spacing="20" alignX="Center">
        <Text content="Valor do Contador: {contador}" size="28" bold="true" color="#ECEFF4" />
        <Row spacing="15" alignY="Center">
            <Button text="Diminuir" onClick="decrementar" color="#BF616A" padding="10 20" />
            <Button text="Aumentar" onClick="incrementar" color="#A3BE8C" padding="10 20" />
        </Row>
    </Column>
</Container>
```

```rust
struct Contador { valor: i32 }

impl Component for Contador {
    fn name(&self) -> &str { "contador" }
    fn template(&self) -> Template { Template::File("templates/contador.xml".into()) }
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

---

## Sumário

- [glacier-ui](#glacier-ui)
  - [Sumário](#sumário)
  - [Por que Glacier](#por-que-glacier)
  - [Instalação](#instalação)
  - [Conceitos e arquitetura](#conceitos-e-arquitetura)
  - [Início rápido](#início-rápido)
  - [Referência de tags](#referência-de-tags)
    - [Layout](#layout)
    - [Conteúdo e controles](#conteúdo-e-controles)
    - [Estruturais (composição, fluxo, recursos)](#estruturais-composição-fluxo-recursos)
  - [Atributos de layout e estilo](#atributos-de-layout-e-estilo)
  - [Data binding e templating](#data-binding-e-templating)
  - [Controle de fluxo](#controle-de-fluxo)
    - [Atributos Diretivas (Recomendado)](#atributos-diretivas-recomendado)
      - [`if` / `else` / `equals` / `notEquals`](#if--else--equals--notequals)
      - [`for-each` e `var`](#for-each-e-var)
      - [Precedência: `for-each` + `if` no mesmo elemento](#precedência-for-each--if-no-mesmo-elemento)
    - [Tags-Invólucro (Legado)](#tags-invólucro-legado)
  - [Inputs de texto](#inputs-de-texto)
  - [Formulários (Reactive Forms)](#formulários-reactive-forms)
  - [Imagens](#imagens)
  - [Componentes e composição](#componentes-e-composição)
    - [`<import>` e referência por nome](#import-e-referência-por-nome)
    - [O trait `Component`](#o-trait-component)
    - [Componentes aninhados e roteamento de ações](#componentes-aninhados-e-roteamento-de-ações)
    - [`ContextVar`](#contextvar)
  - [Navegação entre telas](#navegação-entre-telas)
  - [`<script>` + a macro `#[component]`](#script--a-macro-component)
  - [Stylesheets `.gss`](#stylesheets-gss)
  - [Estilos escopados inline: `<style>` / `style`](#estilos-escopados-inline-style--style)
  - [`<link rel="…">`: stylesheet, import, data, theme](#link-rel-stylesheet-import-data-theme)
  - [Temas](#temas)
  - [Hot-reload](#hot-reload)
  - [Rede e async](#rede-e-async)
  - [Referência da API](#referência-da-api)
    - [`GlacierUI`](#glacierui)
    - [`EngineMessage`](#enginemessage)
    - [Tipos de apoio](#tipos-de-apoio)
  - [Exemplos](#exemplos)
  - [Publicação no crates.io](#publicação-no-cratesio)
  - [Licença](#licença)

---

## Por que Glacier

- **Declarativo de verdade** — a UI é um arquivo XML, não uma árvore de chamadas Rust.
- **Hot-reload** — edite o XML, os estilos `.gss`, os dados JSON ou o tema com a app rodando e veja a mudança na hora; só a lógica em Rust exige recompilar.
- **Data binding por placeholders** — `{chave}` em qualquer atributo, resolvido contra um contexto de estado.
- **Componentes** — encapsulam UI + comportamento + estado num único tipo Rust, compostos por `<import>`, referência por nome ou `children()`.
- **Estilos reutilizáveis** — classes `.gss` (CSS-like) globais ou com escopo por componente, com a mesma precedência do CSS (inline vence classe).
- **Renderiza com `iced`** — widgets nativos, multiplataforma, tema configurável.

---

## Instalação

O projeto é um workspace com dois crates:

- **`glacier-ui`** — o motor.
- **`glacier-ui-macros`** — a proc-macro `#[component]`.

```bash
cargo add glacier-ui
```

`glacier-ui` já puxa `glacier-ui-macros`. As demais dependências (`iced 0.13`,
`roxmltree`, `image`, `serde_json`) vêm junto. Requer Rust **edition 2024**
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
| **`EngineMessage`** | mensagens que o `iced` entrega ao motor (cliques, inputs, navegação, reload). |
| **Stylesheet `.gss`** | classes de estilo reutilizáveis (CSS-like), globais ou por componente. |

O fluxo de cada frame de estado é:

```
XML  ──parse──▶  AST  ──avalia (contexto + estilos + includes + if/ForEach)──▶  AST resolvido  ──render──▶  widgets iced
                                                   ▲                                                            │
                                                   └────────── ação vira EngineMessage, roteada ao Component ◀──┘
```

A integração com o `iced` segue o padrão `application(title, update, view)`:
o `update` da app só repassa a mensagem para `motor.dispatch(...)`, e o `view`
chama `motor.render_current()`.

---

## Início rápido

```rust
use glacier_ui::{GlacierUI, EngineMessage, Component, Context, Template};
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

struct App { motor: GlacierUI }

impl App {
    fn new() -> (Self, Task<EngineMessage>) {
        let mut motor = GlacierUI::new();
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

    fn subscription(&self) -> iced::Subscription<EngineMessage> {
        GlacierUI::reload_subscription(std::time::Duration::from_millis(500))
    }
}

fn main() -> iced::Result {
    iced::application("Contador", App::update, App::view)
        .subscription(App::subscription)
        .run_with(|| App::new())
}
```

A app fica enxuta: registra o componente, repassa as mensagens a `dispatch` e
renderiza com `render_current`. Toda a lógica vive no `Component`.

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
| `<TextInput>` | `Input`, `EntradaTexto` | `placeholder`/`dica`, `value`/`valor`, `onChange`/`aoMudar`, `secure`/`password` (mascara o texto), `formControl` (liga a um `FormControl` pelo nome — veja [Formulários](#formulários-reactive-forms)) |
| `<Form>` | `Formulario` | `onSubmit`/`aoSubmeter`, `name`/`nome` (opcional, só necessário com dois `<Form>`s de controles homônimos no mesmo componente) — renderiza como `<Column>` |
| `<Select>` | `Dropdown`, `PickList`, `ComboBox`, `Seletor` | `options`/`items` (chave de contexto com array JSON), `value`/`valor` (chave com o valor selecionado), `onChange`/`onSelect`, `placeholder`, `labelField` (padrão `label`), `valueField` (padrão `value`), `color`/`cor`. Estilizável via `.gss` (`background`, `border*`, `color`). |
| `<Image>` | `Imagem` | `source`/`src`/`caminho`, `clip="Circle"` (corte circular) |
| `<Svg>` | `Icon`, `Icone` | `source`/`src`, `color`/`cor` (tinge o ícone vetorial) |
| `<Checkbox>` | `Check` | `label`, `checked`/`value` (chave de contexto), `onToggle`/`onChange` |
| `<Toggle>` | `Toggler`, `Switch` | `label`, `checked`/`value`, `onToggle`/`onChange` |
| `<Scrollable>` | `Scroll`, `Rolagem` | `direction`: `vertical` (padrão), `horizontal`, `both` — viewport rolável de 1 filho |
| `<Rule>` | `Divider`, `Divisoria` | `direction`: `horizontal` (padrão) ou `vertical` — linha separadora |

### Estruturais (composição, fluxo, recursos)

| Tag | Aliases | Descrição |
|---|---|---|
| `<import>` | `Importar` | declara um componente carregado de um arquivo: `name`/`nome`, `from`/`de`. |
| `<Include>` | `Incluir` | inclui outro template; demais atributos viram props. |
| `<NomeDoComponente .../>` | — | qualquer tag desconhecida referencia um componente por nome; atributos viram props. |
| `<ForEach>` | `For` | repete os filhos por item: `items`/`itens`, `var`/`variavel`. |
| `<if>` | `Se` | renderiza condicionalmente: `cond`, `equals`, `notEquals`. |
| `<else>` | `Senao` | renderiza quando o `<if>` imediatamente anterior foi falso. |
| `<link>` | `Link` | carrega um recurso: stylesheet, componente, dados ou tema (veja [`<link>`](#link-rel-stylesheet-import-data-theme)). |
| `<style>` | `style`, `Style` | classes `.gss` inline, com escopo no componente (veja [Estilos escopados inline](#estilos-escopados-inline-style--style)). |

---

## Atributos de layout e estilo

Disponíveis em **qualquer** tag:

| Atributo | Aliases | Valores |
|---|---|---|
| `width` | `largura`, `w` | `fill`, `shrink` ou número (px) |
| `height` | `altura`, `h` | `fill`, `shrink` ou número (px) |
| `padding` | `espacamento_interno` | `"10"`, `"10 20"` (vertical horizontal) ou `"10 20 30 40"` (top right bottom left) |
| `alignX` | `align_x`, `alinhamento_x` | `start`, `center`, `end` |
| `alignY` | `align_y`, `alinhamento_y` | `start`, `center`, `end` |
| `spacing` | `espacamento` | número (espaço entre filhos de `Row`/`Column`) |
| `background` | `bg`, `fundo` | cor hex |
| `borderRadius` | `border_radius`, `raio_borda` | número |
| `borderWidth` | `border_width`, `largura_borda` | número |
| `borderColor` | `border_color`, `cor_borda` | cor hex |
| `class` | `classe` | classes `.gss` separadas por espaço (veja [Stylesheets](#stylesheets-gss)) |
| `font` | `fonte`, `font-family` | `mono`/`monospace`/`code` (fonte monoespaçada) ou `bold` — em `Text`/`Button` |
| `gradient` | `gradiente` | gradiente linear de fundo: `"#a #b"` (cima→baixo) ou `"<ângulo> #a #b [#c …]"` (graus); vence `background` |
| `textAlign` | `text_align`, `text-align` | alinhamento horizontal de `Text`: `start`/`center`/`end` |
| `onPress` | `on_press`, `on-press`, `aoPressionar` | ação disparada no **pressionar** (botão do mouse para baixo) sobre o elemento — envolve-o em um `mouse_area`. Diferente do clique de `<Button>` (que dispara ao soltar); a semântica de pressionar é o que viabiliza arrastar a janela (`onPress="window:drag"`). |
| `onDoubleClick` | `on_double_click`, `on-double-click`, `aoClicarDuplo` | ação disparada no **duplo-clique** sobre o elemento (envolve em `mouse_area`). Ex.: duplo-clique na barra de título para maximizar (`onDoubleClick="window:maximize"`). |
| `cursor` | `cursorIcon`, `cursor-icon` | ícone do cursor ao pairar sobre o elemento: `pointer`, `text`, `grab`, `grabbing`, `move`, `crosshair`, `wait`, `progress`, `help`, `not-allowed`, `none`, e os de redimensionar janela `resize-h`/`resize-v`/`resize-ne`/`resize-nw` (envolve em `mouse_area` com a `mouse::Interaction`). |

- **Eixos:** o alinhamento do eixo cruzado de uma `Column` é o `alignX`; o de uma `Row` é o `alignY`.
- **Cores:** hex `#RRGGBB` ou `#RRGGBBAA`.

---

## Ações built-in

Algumas ações de `onClick`/`onPress` são tratadas pelo próprio motor, sem
precisar de código no componente — basta referenciá-las no markup:

| Ação | Efeito |
|---|---|
| `clipboard:<chave>` | copia o valor de contexto `<chave>` para a área de transferência |
| `window:minimize` | minimiza a janela |
| `window:maximize` | alterna maximizar/restaurar (alias `window:toggle_maximize`) |
| `window:close` | fecha a janela |
| `window:drag` | inicia o arraste da janela — use em `onPress` de uma região da barra de título |
| `window:resize:<dir>` | inicia o redimensionamento interativo — `<dir>` ∈ `n`,`s`,`e`,`w`,`ne`,`nw`,`se`,`sw`. Use em `onPress` das alças de borda/canto, junto com `cursor="resize-…"` para o ícone. Requer iced 0.14+. |

As ações `window:*` permitem montar uma barra de título customizada para uma
janela sem decorações (`decorations: false` nas `window::Settings` do iced):

```kdl
Row class="titlebar" width="fill" {
    Row width="fill" on_press="window:drag" {       // região de arraste
        Text "Meu App"
    }
    Button "—" onClick="window:minimize"
    Button "▢" onClick="window:maximize"
    Button "✕" onClick="window:close"
}
```

---

## Data binding e templating

Qualquer valor de atributo pode conter placeholders `{chave}`, substituídos
pelos valores do contexto durante a avaliação:

```xml
<Text content="Olá, {user_name}!" color="{cor_texto}" />
<Container background="{painel_bg}"> ... </Container>
```

O componente publica valores com `ctx.set("user_name", "Clara")` (ou
`motor.define_data("user_name", "Clara")` por fora). Sempre que o contexto muda,
o motor reavalia os templates e a UI reflete o novo valor. **Chaves ausentes
viram string vazia.**

---

## Controle de fluxo

A forma recomendada de controlar o fluxo (condicionais e loops) é através de **atributos diretivas** aplicados diretamente em qualquer elemento (estilo Vue/Angular). Também é suportada a sintaxe antiga de tags-invólucro `<if>`, `<else>` e `<ForEach>` por retrocompatibilidade.

### Atributos Diretivas (Recomendado)

#### `if` / `else` / `equals` / `notEquals`

Você pode usar o atributo `if` em qualquer elemento para renderizá-lo condicionalmente. O atributo `else` (pelado) se conecta ao `if` anterior.

```xml
<!-- Checagem truthy simples (true / 1 / yes / on / sim) -->
<Column if="{logado}">
    <Text content="Bem-vindo!" />
</Column>
<Column else>
    <Text content="Por favor, conecte-se." />
</Column>

<!-- Comparação explícita de igualdade ou diferença -->
<Text content="Painel Admin" if="{papel}" equals="admin" />
<Text content="Acesso Comum" if="{papel}" notEquals="admin" />
```

*Nota sobre XML estrito:* Atributos pelados como `else` não são aceitos pelo padrão XML. O Glacier UI faz um pré-processamento transparente convertendo `else` para `else=""` antes do parseamento. Ambos `else` e `senao` são suportados.

#### `for-each` e `var`

O atributo `for-each` itera sobre um **array JSON** publicado no contexto. Use o atributo `var` para definir a variável do loop (padrão é `item` se omitido). Cada item vira variáveis prefixadas pelo nome declarado em `var` (`u.nome`, `u.cargo`, etc.); strings/números simples ficam disponíveis diretamente como `{u}`:

```xml
<CartaoUsuario
    for-each="usuarios"
    var="u"
    nome="{u.nome}"
    cargo="{u.cargo}"
    cor="{u.cor}"
/>
```

```rust
let json = serde_json::json!([
    { "nome": "Clara",  "cargo": "Engenheira", "cor": "#89B4FA" },
    { "nome": "Sophia", "cargo": "Designer",   "cor": "#F5C2E7" },
]).to_string();
ctx.set("usuarios", json);
```

#### Precedência: `for-each` + `if` no mesmo elemento

Quando combinados no mesmo elemento, o `for-each` tem precedência maior. Ele desenrola o loop primeiro e depois o `if` filtra individualmente cada item gerado usando o contexto local do loop.

---

### Tags-Invólucro (Legado)

Abaixo está a sintaxe legada baseada em tags, mantida para retrocompatibilidade:

```xml
<!-- Condicional legada -->
<if cond="{logado}">
    <Text content="Olá, {usuario}" />
</if>
<else>
    <Text content="Entre" />
</else>

<!-- Loop legado -->
<ForEach items="usuarios" var="u">
    <CartaoUsuario nome="{u.nome}" cargo="{u.cargo}" />
</ForEach>
```

Veja `examples/condicional.rs` e `examples/lista.rs`.

---

## Inputs de texto

`<TextInput>` faz binding bidirecional: `value` aponta para a chave de contexto
exibida, e `onChange` dispara uma ação com o novo texto a cada digitação. No
`update`, o texto chega em `value: Option<&str>`:

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

Veja `examples/perfil.rs` e `examples/navegacao.rs`.

---

## Formulários (Reactive Forms)

Inspirado no Angular Reactive Forms: `FormBuilder` declara os `FormControl`s
(nome, valor inicial, validadores) do lado Rust; o componente guarda o `Form`
construído no seu próprio estado; e o template liga cada input a um controle
pelo atributo `formControl` — o motor cuida do resto (o texto digitado vai
para o controle certo, Enter sempre submete e avança para o próximo campo).

```rust
use glacier_ui::{Form, FormBuilder, FormControl};

struct Login {
    form: Form,
}

impl Login {
    fn novo() -> Self {
        Self {
            form: FormBuilder::new("login")
                .control(FormControl::new("username", "").required().min_length(3))
                .control(FormControl::new("password", "").required().min_length(6))
                .build(),
        }
    }
}
```

```kdl
Form onSubmit="entrar" name="login" {
    TextInput formControl="username" placeholder="usuário"
    TextInput formControl="password" placeholder="senha" secure=true
    Button text="Entrar" onClick="entrar"
}
```

`TextInput formControl="username"` sem `value`/`onChange` explícitos usa o
nome do controle para os dois — ele lê `username` do contexto e dispara a
ação `"username"` a cada tecla. No `update`, `Form::has_control` reconhece
essa ação sem precisar de um `match` por campo:

```rust
fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context) {
    if self.form.has_control(action) {
        self.form.set_value(action, value.unwrap_or_default());
        self.form.sync_to_context(ctx); // republica valores/estado no contexto
        return;
    }
    if action == "entrar" {
        if self.form.is_valid() {
            // ... prossegue com self.form.value("username") etc.
        } else {
            self.form.validate(); // mostra erros também nos campos não tocados
            self.form.sync_to_context(ctx);
        }
    }
}
```

Validadores disponíveis em `FormControl`: `.required()`, `.min_length(n)`,
`.max_length(n)`, `.pattern(regex)` e `.validator(|valor| Ok(()) | Err(msg))`
para qualquer regra própria. `Form::is_valid()` é sempre recalculado na hora
(seguro chamar antes de qualquer edição, ex. num botão desabilitado);
`Form::validate()` força a checagem e marca todo campo como tocado, útil num
handler de submit para exibir erros em campos que o usuário nunca editou.
`Form::errors(nome)` devolve as mensagens do último `validate`/`set_value`.

Pressionar Enter num campo ligado a um `formControl` **sempre** dispara o
`onSubmit` do `<Form>` (quem decide o que fazer é o `update()`, via
`Form::is_valid()` — o motor não bloqueia a submissão de um form inválido); se
houver um próximo campo no mesmo `<Form>`, o foco também avança para ele, como
um Tab automático — dá para preencher e enviar o formulário inteiro sem tocar
no mouse.

Veja `examples/formulario_login.rs` e `templates/formulario_login.kdl`.

---

## Imagens

`<Image>` carrega um arquivo de imagem; `clip="Circle"` recorta em círculo
(ótimo para avatares). `width`/`height` definem o tamanho.

```xml
<Image source="templates/avatar.png" width="100" height="100" clip="Circle" />
```

Veja `templates/perfil_card.xml`.

---

## Componentes e composição

### `<import>` e referência por nome

Há duas formas de compor UI **a nível de template**:

**1. `<import>` + referência por nome** (recomendado):

```xml
<import name="PerfilCard" from="templates/perfil_card.xml" />
<Container>
    <PerfilCard nome="{user_name}" cargo="{user_role}" />
</Container>
```

**2. `<Include src="..." />`** — inclusão equivalente, com os demais atributos
como props.

Em ambos os casos os atributos viram **props**, interpoladas no contexto local
do componente incluído — então o filho usa `{nome}`, `{cargo}` etc. O `<import>`
pode aparecer no topo do arquivo, como irmão da raiz.

> Também é possível carregar um componente declarativamente via
> `<link rel="import" .../>` — veja [`<link>`](#link-rel-stylesheet-import-data-theme).

### O trait `Component`

Encapsula **UI + comportamento + estado** num único tipo Rust:

```rust
pub trait Component {
    fn name(&self) -> &str;                  // nome (registro + roteamento)
    fn template(&self) -> Template;          // Template::File(path) | Template::Inline(xml)
    fn init(&mut self, ctx: &mut Context) {} // estado inicial (opcional)
    fn children(&self) -> Vec<Box<dyn Component>> { Vec::new() } // sub-componentes (opcional)
    fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context);
}
```

- `update` recebe a ação (`onClick`/`onChange`); `value` vem preenchido em inputs e é `None` em cliques.
- `Context` expõe `get`, `set`, `set_var`, `navigate_to`, `navigate_back`.
- O estado próprio mora nos campos do struct; o contexto guarda só a projeção string usada pelos templates.

Registre com `motor.register(Box::new(MeuComponente { .. }))`: o motor parseia o
template, processa `<import>`/`<link>`, chama `init` e passa a rotear ações para
o `update`.

> Há também a API de baixo nível `register_component(name, path)`, que registra
> **só a UI** (o comportamento fica no `update()` do app). As duas convivem.

### Componentes aninhados e roteamento de ações

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
    fn update(&mut self, action: &str, _v: Option<&str>, ctx: &mut Context) { /* ... */ }
}
```

No `painel.xml`, basta referenciar `<CartaoContador />` — sem `<import>`, porque
o `children()` já registrou o template e o comportamento do filho.

**Como o roteamento funciona:** ao inlinar a subárvore de `<CartaoContador />`,
o motor prefixa as ações daquela subárvore com o nome do componente
(`incrementar` → `CartaoContador::incrementar`). No `dispatch`:

- prefixo que corresponde a um componente com comportamento → vai para ele;
- ação sem prefixo, ou prefixo só de UI (sem `Component` registrado) → cai na **tela ativa** (fallback). É isso que mantém includes puramente visuais funcionando.

> **Limite conhecido:** um filho referenciado N vezes (ex.: dentro de
> `<ForEach>`) compartilha um único objeto e um único `update` — ótimo para
> filhos sem estado próprio. Veja `examples/aninhado.rs` e `examples/lista.rs`.

### `ContextVar`

Açúcar para declarar variáveis de contexto de forma legível, em vez de chaves
string soltas:

```rust
fn init(&mut self, ctx: &mut Context) {
    ctx.set_var(&ContextVar::new("user_name", "Clara Silva"));
    ctx.set_var(&ContextVar::new("btn_color", "#313244"));
}
```

`ctx.set("chave", "valor")` (forma direta) continua disponível. Veja
`examples/perfil.rs`.

---

## Navegação entre telas

Cada tela é um componente registrado. Botões declaram o destino no próprio XML:

```xml
<Button text="Abrir perfil" navigateTo="perfil" />
<Button text="Voltar" navigateBack="true" />
```

O motor mantém uma **pilha de histórico**: `navigateTo` empilha a tela atual e
troca para o destino; `navigateBack` volta para a anterior. O estado de contexto
é **compartilhado** entre telas — o que você edita numa aparece na outra.

No código:

```rust
motor.set_initial_screen("home"); // tela inicial, limpa o histórico
motor.navigate_to("perfil");      // empilha a atual
motor.navigate_back();            // volta à anterior
```

Componentes também podem pedir navegação de dentro do `update` via
`ctx.navigate_to(...)` / `ctx.navigate_back()`. Veja `examples/navegacao.rs`.

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
use glacier_ui::component;

#[component(path = "templates/contador_macro.xml", name = "contador")]
#[derive(Default)]
struct Contador { contador: i32 }
```

A macro gera:

- cada `fn` do `<script>` vira um método **e** uma ação de mesmo nome (casada com `onClick`/`onChange`);
- um método com argumento extra recebe o valor do input (`fn mudar(&mut self, v: &str)`);
- cada campo nomeado do struct é sincronizado com o contexto (`to_string()`) no `init` e após cada ação — por isso `{contador}` reflete `self.contador`.

O `<script>` é removido **antes** do parse XML, então pode ficar como irmão da
raiz sem invalidar o documento.

**Tradeoff:** é Rust real, type-checado pelo compilador, mas mudar a lógica do
`<script>` exige **recompilar** (o markup continua com hot-reload). Veja
`examples/contador_macro.rs`.

---

## Stylesheets `.gss`

Um `.gss` (*glacier stylesheet*) é um arquivo CSS-like que tira estilos repetidos
da markup e os agrupa em **classes** reutilizáveis. Aplique-as com
`class="..."`:

```gss
// styles/app.gss
/* Comentários de linha (//) e de bloco (/* ... */, multilinha) são suportados.
   '#' nunca é comentário, então cores #RRGGBB ficam intactas. */

.card {
    padding: 24;
    background: #1E1E2E;
    border-radius: 16;
    border-width: 1;
    border-color: #313244;
    align-x: Center;
}

.title { size: 26; bold: true; color: #CDD6F4; }
```

```xml
<Container class="card">
    <Text class="title" content="Olá" />
</Container>
```

**Precedência (igual à do CSS):**

1. um **atributo inline** no nó sempre vence a classe;
2. classes são aplicadas da **esquerda para a direita** (`class="a b"` → `b` sobrepõe `a`);
3. estilos **globais** primeiro, depois os **com escopo** do componente (veja `<link>`).

**Propriedades reconhecidas:** `width`/`w`, `height`/`h`, `padding`, `spacing`,
`align-x`/`align_x`/`alignX`, `align-y`/`align_y`/`alignY`, `background`/`bg`,
`border-radius`, `border-width`, `border-color`, `color`, `size`, `bold`.

Carregue uma stylesheet **global** por código:

```rust
motor.load_stylesheet("styles/app.gss")?; // vale para todos os componentes
```

…ou uma **com escopo** via `<link>` no template (próxima seção). Veja
`examples/estilos.rs`.

---

## Estilos escopados inline: `<style>` / `style`

Além de carregar um `.gss` externo, você pode escrever as classes **direto no
template**, num bloco `<style>`. O conteúdo é `.gss` (mesma gramática) e fica
**com escopo no componente** que o declarou — exatamente como um
`<link rel="stylesheet">`, mas sem arquivo separado. Ótimo para estilos
específicos de uma página/cartão que não valem reutilizar.

```xml
<!-- XML: o corpo do <style> é GSS, escopado a este componente -->
<style>
    .card  { padding: 24; background: #1E1E2E; border-radius: 16; }
    .title { size: 26; bold: true; color: #CDD6F4; }
</style>

<Container class="card">
    <Text class="title" content="Olá" />
</Container>
```

Em **KDL** não há tag de fechamento; use uma *string multilinha* (`"""…"""`)
como argumento do nó `style`. O parser distingue **inline** de **arquivo** pela
presença de `{`/quebra de linha (um caminho não tem nenhum dos dois):

```kdl
// inline: corpo GSS numa string multilinha
style """
.card  { padding: 24; background: #1E1E2E; border-radius: 16; }
.title { size: 26; bold: true; color: #CDD6F4; }
"""

// arquivo externo (continua válido): vira um stylesheet link
style "styles/estilos.gss"

Container class="card" {
    Text "Olá" class="title"
}
```

- **Escopo:** as classes só valem na subárvore do componente declarante, **em
  cima** das globais — uma classe inline de mesmo nome vence a global,
  localmente (igual ao `<link rel="stylesheet">`).
- **Ordem de documento:** se um componente tiver `<link>` e `<style>` (ou
  vários), eles se sobrepõem na ordem em que aparecem — o **último vence** num
  empate de propriedade.
- **Hot-reload:** como o `<style>` mora no template, editar o bloco recarrega
  junto com o markup, sem recompilar.
- Como os `<import>`/`<link>`, um `<style>` pode ficar no topo do arquivo, como
  irmão da raiz, e não renderiza nada.

---

## `<link rel="…">`: stylesheet, import, data, theme

O `<link>` declara um recurso externo a carregar. O atributo `rel` escolhe o
tipo:

| `rel` | O que faz | Escopo | Atributos |
|---|---|---|---|
| `stylesheet` (padrão) | carrega um `.gss` | **por componente** | `href` |
| `import` / `component` | carrega outro template (igual a `<import>`) | global | `href`, `as`/`name` (default = nome do arquivo) |
| `data` | faz merge de um JSON no contexto | global | `href`, `as`/`name` (obrigatório) |
| `theme` | aplica uma paleta como `iced::Theme` | global (app) | `href` |

```xml
<!-- stylesheet COM ESCOPO: as classes só valem dentro deste componente -->
<link rel="stylesheet" href="styles/estilos.gss" />

<!-- carregar um componente declarativamente -->
<link rel="import" href="templates/perfil_card.xml" as="PerfilCard" />

<!-- injetar dados de um JSON no contexto -->
<link rel="data" href="data/equipe.json" as="app" />

<!-- definir o tema da janela -->
<link rel="theme" href="styles/theme.json" />
```

- **`stylesheet`** — a sheet vale só na subárvore do componente que a declarou, **em cima** das globais (uma classe escopada de mesmo nome vence a global, localmente).
- **`import`/`component`** — equivalente declarativo do `<import>`; reusa o registro recursivo. Sem `as`, o nome vem do *stem* do arquivo.
- **`data`** — lê e valida o JSON e faz merge no contexto: um **objeto** vira chaves `app.campo`; um **array** ou escalar fica em `app`. Isso alimenta `{app.campo}` e `<ForEach items="app.lista">`.
- **`theme`** — veja a próxima seção.

Como os `<import>`, os `<link>` podem ficar no topo do arquivo, como irmãos da
raiz, e não renderizam nada.

---

## Temas

`<link rel="theme" href="...">` carrega um JSON de cores e o aplica como
`iced::Theme`. O JSON tem cores hex e um `name` opcional:

```json
{
    "name": "Mocha",
    "background": "#181825",
    "text": "#CDD6F4",
    "primary": "#89B4FA",
    "success": "#A6E3A1",
    "danger": "#F38BA8"
}
```

O motor expõe o tema por `motor.theme()` (que devolve `Theme::Dark` se nenhum
foi carregado). Ligue-o na sua `application`:

```rust
impl AppEstilos {
    fn theme(&self) -> iced::Theme { self.motor.theme() }
}

iced::application("Glacier", AppEstilos::update, AppEstilos::view)
    .subscription(AppEstilos::subscription)
    .theme(AppEstilos::theme)   // <- aplica o tema do <link rel="theme">
    .run_with(|| AppEstilos::new())
```

> Definir um tema também resolve o "fundo branco" padrão do `iced` em áreas que
> o seu layout não cobre. Veja `examples/estilos.rs`.

---

## Hot-reload

Recursos carregados de arquivo são recarregados quando mudam em disco:

- **templates** (`Template::File`) — inclusive `<import>`/`<link>` novos;
- **stylesheets `.gss`** — globais e com escopo;
- **dados** (`<link rel="data">`) — re-merge no contexto;
- **tema** (`<link rel="theme">`) — re-aplicado no próximo redraw.

Ligue a subscription do `iced` ao motor:

```rust
fn subscription(&self) -> iced::Subscription<EngineMessage> {
    GlacierUI::reload_subscription(std::time::Duration::from_millis(500))
}

fn update(&mut self, msg: EngineMessage) -> Task<EngineMessage> {
    // dispatch trata EngineMessage::FileChanged chamando check_reload()
    let _ = self.motor.dispatch(&msg);
    Task::none()
}
```

Edite o XML, o `.gss`, o JSON de dados ou o tema e veja a UI atualizar sem
recompilar. (A lógica de um `<script>` é a exceção — veja o tradeoff da macro.)

---

## Rede e async

Um componente não fica preso à thread de UI: ele pode disparar I/O (rede, disco,
timers) por **efeitos** e receber fluxos contínuos por **subscriptions**. O
motor faz a ponte com o executor do `iced`.

**Efeitos pontuais** — dentro do `update`, chame `ctx.perform(future)`. Quando o
future completa, seus pares `(chave, valor)` são mesclados no contexto e a UI é
reavaliada:

```rust
fn update(&mut self, action: &str, _v: Option<&str>, ctx: &mut Context) {
    if action == "carregar" {
        ctx.set("status", "carregando…");
        ctx.perform(async {
            let corpo = buscar_do_servidor().await;
            vec![("status".into(), "ok".into()), ("corpo".into(), corpo)]
        });
    }
}
```

Para isso, `dispatch` agora devolve uma `iced::Task<EngineMessage>` — repasse-a
no `update` da app:

```rust
fn update(&mut self, msg: EngineMessage) -> Task<EngineMessage> {
    self.motor.dispatch(&msg)   // efeitos viram Tasks automaticamente
}
```

**Fluxos contínuos (sockets, watchers)** — implemente `Component::subscription`
devolvendo uma `iced::Subscription` que emita
`EngineMessage::ContextPatch(pares)`. O motor agrega as subscriptions de todos os
componentes em `GlacierUI::subscription`, que você liga à sua app:

```rust
fn subscription(&self) -> iced::Subscription<EngineMessage> {
    Subscription::batch([
        self.motor.subscription(),                       // componentes (rede)
        GlacierUI::reload_subscription(Duration::from_millis(500)), // hot-reload
    ])
}
```

Cada item recebido vira um `ContextPatch`: o motor mescla os pares no contexto e
reavalia a UI. Assim um daemon remoto, um stream de logs ou métricas atualizam a
tela sem você escrever nenhum `match` de mensagens.

> `EngineMessage::ContextPatch(Vec<(String, String)>)` também pode ser produzido
> pela própria app e repassado a `dispatch` — é a porta de entrada genérica para
> empurrar estado externo para dentro do contexto.

---

## Referência da API

### `GlacierUI`

| Método | Descrição |
|---|---|
| `new()` | cria um motor vazio. |
| `register(Box<dyn Component>)` | registra um componente (UI + comportamento + `children()` em cascata). |
| `register_component(name, path)` | API de baixo nível: registra só a UI a partir de um arquivo. |
| `load_stylesheet(path)` | carrega/recarrega um `.gss` **global** e reavalia tudo. |
| `theme()` | o `iced::Theme` do `<link rel="theme">`, ou `Theme::Dark`. |
| `dispatch(&EngineMessage)` | roteia a mensagem ao componente dono, aplica navegação/reload/patch e devolve uma `iced::Task` com os efeitos pedidos. |
| `subscription()` | agrega as `Component::subscription` de todos os componentes numa só `iced::Subscription`. |
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
    UiClick(String),                                   // onClick
    UiInputChanged { action: String, value: String },  // onChange
    Navigate(String),                                  // navigateTo
    NavigateBack,                                      // navigateBack
    FileChanged(String),                               // tick do hot-reload
    ContextPatch(Vec<(String, String)>),               // efeitos/subscriptions -> contexto
    UiSubmit { action: String, next_focus: Option<String> }, // Enter num formControl
    // ... DragStart/DragHover/DragEnd (drag-and-drop) e UiEditorAction (TextArea)
}
```

### Tipos de apoio

- `Template::File(String)` | `Template::Inline(String)`
- `Context` — `get`, `set`, `set_var`, `navigate_to`, `navigate_back`
- `ContextVar::new(key, value)`
- `Nav::To(String)` | `Nav::Back`
- `FormBuilder::new(nome).control(FormControl::new(nome, valor_inicial)...)build()`
- `Form` — `get`/`get_mut`, `has_control`, `value`/`set_value`, `errors`, `is_valid`, `validate`, `reset`, `control_names`, `values`, `sync_to_context`
- `FormControl` — `.required()`, `.min_length(n)`, `.max_length(n)`, `.pattern(regex)`, `.validator(f)`

---

## Exemplos

| Exemplo | Demonstra | Rodar |
|---|---|---|
| `contador` | `Component` básico com estado e cliques. | `cargo run --example contador` |
| `contador_macro` | comportamento embutido no XML via `<script>` + `#[component]`. | `cargo run --example contador_macro` |
| `perfil` | inputs (`TextInput`), cliques, `<import>` de um cartão, `Image` e `ContextVar`. | `cargo run --example perfil` |
| `lista` | `<ForEach>` sobre JSON com um componente (`<import>`) por item. | `cargo run --example lista` |
| `condicional` | `<if>` / `<else>` (truthy e comparação). | `cargo run --example condicional` |
| `navegacao` | múltiplas telas, histórico e `navigateTo`/`navigateBack` com estado compartilhado. | `cargo run --example navegacao` |
| `aninhado` | componente registrado dentro de outro (`children()`), com roteamento por namespace. | `cargo run --example aninhado` |
| `estilos` | stylesheets `.gss` (globais e com escopo via `<link>`), classes e tema. | `cargo run --example estilos` |
| `estilos_inline` | classes `.gss` inline e com escopo via bloco `<style>` (XML). | `cargo run --example estilos_inline` |
| `estilos_inline_kdl` | o mesmo em KDL, com o corpo GSS numa string multilinha de `style`. | `cargo run --example estilos_inline_kdl` |
| `formulario_login` | `Form`/`FormBuilder`/`FormControl`: validação, Enter para submeter/avançar campo. | `cargo run --example formulario_login` |

---

## Publicação no crates.io

O workspace publica em duas etapas (a proc-macro primeiro, pois o motor depende
dela):

```bash
cargo login                          # token de https://crates.io/settings/tokens
cargo publish -p glacier-ui-macros   # 1) a macro
cargo publish -p glacier-ui          # 2) o motor (após o índice atualizar)
```

Valide antes com `cargo publish --dry-run`.

---

## Licença

Licenciado sob **MIT OR Apache-2.0**, à sua escolha.
