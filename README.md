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
            <Button text="Diminuir" on_click="decrementar" color="#BF616A" padding="10 20" />
            <Button text="Aumentar" on_click="incrementar" color="#A3BE8C" padding="10 20" />
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
  - [Ações built-in](#ações-built-in)
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
  - [`<script>` em Lua](#script-em-lua)
    - [Rede: `fetch` (async/await via corrotina)](#rede-fetch-asyncawait-via-corrotina)
    - [Imports / módulos: `require`](#imports--módulos-require)
  - [Stylesheets `.gss`](#stylesheets-gss)
  - [Estilos inline: `<style>` / `<style scoped="true">`](#estilos-inline-style--style-scopedtrue)
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

O projeto é um único crate, **`glacier-ui`** — o motor.

```bash
cargo add glacier-ui
```

As dependências (`iced 0.14`, `roxmltree`, `image`, `serde_json`, `mlua` com
Lua 5.4 vendorizado, e `hyper` + `rustls` para o `fetch`) vêm junto — o Lua é
compilado a partir do fonte, sem precisar de Lua no sistema. Requer Rust
**edition 2024** (≥ 1.85).

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
| `<Button>` | `button`, `Botao` | `text`/`texto`, `on_click`/`aoClicar`, `navigateTo`/`irPara`, `navigateBack`/`voltar`, `color`/`cor` |
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
| `<style>` | `style`, `Style` | classes `.gss` inline, global por padrão, ou escopado com `scoped="true"` (veja [Estilos inline](#estilos-inline-style--style-scopedtrue)). |

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
| `hidden` | `oculto` | `true`/`false` — remove o elemento do layout (não ocupa espaço nem `spacing`), também disponível via classe `.gss` (`hidden: true` / `display: none`). |
| `disabled` | `desabilitado` | `true`/`false` — desativa a interação de `Button`/`TextInput`/`Checkbox`/`Toggle` (sem handler anexado, o `Status::Disabled` nativo do iced entra em vigor sozinho). Só existe como atributo inline; veja [Pseudo-estados](#pseudo-estados-hover--focus--active--disabled). |

- **Eixos:** o alinhamento do eixo cruzado de uma `Column` é o `alignX`; o de uma `Row` é o `alignY`.
- **Cores:** hex `#RRGGBB` ou `#RRGGBBAA`.

---

## Ações built-in

Algumas ações de `on_click`/`onPress` são tratadas pelo próprio motor, sem
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

```xml
<Row class="titlebar" width="fill">
    <Row width="fill" onPress="window:drag">      <!-- região de arraste -->
        <Text content="Meu App" />
    </Row>
    <Button text="—" on_click="window:minimize" />
    <Button text="▢" on_click="window:maximize" />
    <Button text="✕" on_click="window:close" />
</Row>
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
    fn new() -> Self {
        Self {
            form: FormBuilder::new("login")
                .control(FormControl::new("username", "").required().min_length(3))
                .control(FormControl::new("password", "").required().min_length(6))
                // A lógica de submissão fica declarada junto com os controles.
                .on_submit(|form, ctx| {
                    if form.is_valid() {
                        ctx.set("status", format!("Bem-vindo, {}!", form.value("username")));
                    } else {
                        form.validate(); // mostra erros também nos campos não tocados
                        form.errors_to_context(ctx, "erro_");
                    }
                })
                .build(),
        }
    }
}
```

```xml
<Form onSubmit="entrar" name="login" width="fill">
    <TextInput formControl="username" placeholder="usuário" width="fill" />
    <TextInput formControl="password" placeholder="senha" secure="true" width="fill" />
    <Button text="Entrar" on_click="entrar" />
</Form>
```

`TextInput formControl="username"` sem `value`/`onChange` explícitos usa o
nome do controle para os dois — ele lê `username` do contexto e dispara a
ação `"username"` a cada tecla. No `update`, `Form::has_control` reconhece
essa ação sem precisar de um `match` por campo — e a submissão (`onSubmit`)
**não** passa por `update`, e sim por um método próprio do trait,
`on_form_submit`, então atualização de campo e submissão nunca competem pelo
mesmo `match`:

```rust
fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context) {
    if self.form.has_control(action) {
        self.form.set_value(action, value.unwrap_or_default());
        self.form.sync_to_context(ctx); // republica valores no contexto
    }
}

fn on_form_submit(&mut self, _action: &str, ctx: &mut Context) {
    self.form.submit(ctx); // roda a closure registrada em .on_submit(...)
}
```

Validadores disponíveis em `FormControl`: `.required()`, `.min_length(n)`,
`.max_length(n)`, `.pattern(regex)` e `.validator(|valor| Ok(()) | Err(msg))`
para qualquer regra própria. `Form::is_valid()` é sempre recalculado na hora
(seguro chamar antes de qualquer edição, ex. num botão desabilitado);
`Form::validate()` força a checagem e marca todo campo como tocado, útil num
handler de submit para exibir erros em campos que o usuário nunca editou.
`Form::errors(nome)` devolve as mensagens do último `validate`/`set_value`;
`Form::errors_to_context(ctx, prefixo)` publica a primeira de cada campo de
uma vez (`"{prefixo}{nome}"`), para inputs `Text "{erro_username}"` inline.

Pressionar Enter num campo ligado a um `formControl` **sempre** dispara o
`onSubmit` do `<Form>` (quem decide o que fazer é `on_form_submit` (ou a
closure de `.on_submit()`), via `Form::is_valid()` — o motor não bloqueia a
submissão de um form inválido); se houver um próximo campo no mesmo `<Form>`,
o foco também avança para ele, como um Tab automático — dá para preencher e
enviar o formulário inteiro sem tocar no mouse.

**Attributos `width`/`padding` de widgets com default próprio do `iced`:**
`<TextInput>` já nasce com `width: fill` e algum padding no `iced` — se o
template não define `width`/`padding`, o glacier preserva esse default (em vez
de forçar `Shrink`/zero). Mas `Length::Fill` só "enche" se todo container pai
até ele também for `width=fill` (`<Column>`, `<Form>` etc. têm default
`Shrink`, como a maioria dos widgets) — a mesma regra já vale pra `<Row>` (veja
a nota de layout abaixo). Por isso o `<Form>` do exemplo e os `<Column>` que
envolvem cada campo também levam `width=fill`.

Veja `examples/formulario_login/` (`main.rs` + `formulario_login.xml`).

---

## Imagens

`<Image>` carrega um arquivo de imagem; `clip="Circle"` recorta em círculo
(ótimo para avatares). `width`/`height` definem o tamanho.

```xml
<Image source="templates/avatar.png" width="100" height="100" clip="Circle" />
```

Veja `examples/perfil/perfil_card.xml`.

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
    fn on_form_submit(&mut self, action: &str, ctx: &mut Context) {} // onSubmit de um <Form> (opcional)
}
```

- `update` recebe a ação (`on_click`/`onChange`); `value` vem preenchido em inputs e é `None` em cliques.
- `on_form_submit` recebe só o `onSubmit` de um `<Form>` (veja [Formulários](#formulários-reactive-forms)) — nunca passa por `update`, então atualização de campo e submissão não competem pelo mesmo `match`.
- `Context` expõe `get`, `set`, `set_var`, `navigate_to`, `navigate_back`.
- O estado próprio mora nos campos do struct; o contexto guarda só a projeção string usada pelos templates.

Registre com `motor.register(Box::new(MeuComponente { .. }))`: o motor parseia o
template, processa `<import>`/`<link>`, chama `init` e passa a rotear ações para
o `update`.

> Há também `register_component(name, path)`, que registra um componente a
> partir de um arquivo: se o template tiver um `<script>`, o comportamento
> **Luau** é ligado automaticamente; senão fica só a UI (o comportamento, se
> houver, vem do `update()` do app ou de um `Component` em Rust via `register`).

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

## `<script>` em Lua

Dá para colocar o comportamento **dentro do próprio template**, num bloco
`<script>` com funções **Lua** (5.4). Ao registrar o template com
`register_component`, o motor detecta o `<script>`, carrega o script e roteia
cada ação (`on_click`/`onChange`/`onSubmit`) para a função de mesmo nome — tudo
**interpretado em tempo de execução**, sem recompilar o app.

```xml
<!-- examples/contador_macro/contador_macro.xml -->
<Container ...>
    <Text content="Valor: {contador}" />
    <Button text="+" on_click="incrementar" />
    <Button text="-" on_click="decrementar" />
</Container>

<script>
function init()        ctx.contador = ctx.contador or 0 end
function incrementar() ctx.contador = ctx.contador + 1 end
function decrementar() ctx.contador = ctx.contador - 1 end
</script>
```

```rust
let mut motor = GlacierUI::new();
motor.register_component("contador", "examples/contador_macro/contador_macro.xml")?;
```

Como funciona:

- cada função Lua vira uma ação de mesmo nome (casada com `on_click`/`onChange`/`onSubmit`);
- o **contexto** do motor é a tabela global `ctx`: ler `ctx.contador` devolve o valor atual e atribuir `ctx.contador = ...` grava de volta. Como Lua coage strings numéricas em aritmética, `ctx.contador + 1` sobre `"0"` volta como `"1"`. Atribuir `ctx.x = nil` **remove** a chave do contexto;
- ações de `onChange` recebem o texto digitado como 1º argumento **e** na global `value` (`function set_nome(v) ctx.nome = v end`);
- a função opcional `init()` semeia o estado inicial.

Depois de cada chamada, a tabela `ctx` é copiada de volta ao contexto e os
bindings `{contador}` refletem na próxima avaliação. O `<script>` é removido
**antes** do parse XML, então pode ficar como irmão da raiz sem invalidar o
documento.

**Arquivo Lua externo:** em vez de embutir o código, aponte para um `.luau`
separado com `src` (ou `from`) — o caminho é resolvido relativo ao diretório do
template:

```xml
<script src="contador.luau"></script>
```

**Vantagem:** mudar a lógica do `<script>` não exige recompilar — junto com o
hot-reload do markup, a UI e o comportamento iteram sem build. Veja
`examples/contador_macro.rs` (inline), `examples/contador_externo.rs` (arquivo
externo) e o módulo `glacier_ui::luau`.

### Rede: `fetch` (async/await via corrotina)

O Lua tem uma função global `fetch(url, opts)` para chamadas HTTP/HTTPS (via
`hyper` + `rustls`). Ela **suspende a corrotina** da ação e retoma quando a
resposta chega — a UI **não trava** durante a requisição, mas o código Lua fica
com cara de `await`, síncrono e linear:

```luau
function buscar()
    ctx.status = "carregando..."                 -- já aparece na tela
    local res = fetch("https://api.exemplo/dados") -- suspende aqui, sem bloquear
    if res.ok then
        ctx.dados = res.body                       -- retomou com a resposta
    else
        ctx.erro = res.error
    end
end
```

O resultado é uma tabela `{ ok, status, body, error }`. O 2º argumento `opts` é
opcional: `{ method = "POST", body = "...", headers = { ["Authorization"] = "..." } }`.
Como cada `fetch` volta ao ponto exato onde parou, dá pra encadear várias em
sequência. Veja `examples/fetch_luau.rs`.

### Imports / módulos: `require`

Para não amontoar tudo num `<script>`, extraia lógica em **bibliotecas** `.luau`
e importe com `require` — um client de rede, utilitários, etc., cada peça
encapsulada e reutilizável entre componentes:

```luau
-- net/http_client.luau — a lógica de rede mora aqui, isolada
local Client = {}
Client.__index = Client
function Client.new(base) return setmetatable({ base = base }, Client) end
function Client:get(path) return fetch(self.base .. path) end  -- usa o fetch async
return Client
```

```luau
-- <script> do template
local http = require("net.http_client")   -- net/http_client.luau
local api  = http.new("https://api.exemplo")

function carregar()
    local res = api:get("/dados")          -- suspende a corrotina por baixo
    if res.ok then ctx.dados = res.body end
end
```

`require("a.b")` procura `a/b.luau` (e depois `a/b/init.luau`) nas raízes, **nesta
ordem**:

1. o **diretório do template** (mesma convenção do `src=`);
2. um subdiretório **`lib/`** desse diretório (para código compartilhado);
3. cada caminho em **`GLACIER_LUA_PATH`** (separados por `:`), para bibliotecas
   fora da árvore do template.

Detalhes: o módulo roda no **mesmo** interpretador do componente, então enxerga
`fetch` e as globais — um client importado pode suspender a ação como qualquer
código inline. Cada módulo é carregado **uma vez** e cacheado (como no Lua
padrão); um módulo sem `return` vira `true`. Módulos são carregados no
*startup* do componente, então editar um `.luau` importado pede reiniciar o app
(o hot-reload observa só o template). Veja `examples/imports_luau/`.

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
3. estilos **globais** primeiro, depois os **com escopo** do componente (`<style scoped="true">`).

**Propriedades reconhecidas:** `width`/`w`, `height`/`h`, `padding`, `spacing`,
`align-x`/`align_x`/`alignX`, `align-y`/`align_y`/`alignY`, `background`/`bg`,
`border-radius`, `border-width`, `border-color`, `color`, `size`, `bold`.

### Pseudo-estados: `:hover` / `:focus` / `:active` / `:disabled`

Além da regra base `.classe { }`, uma classe pode declarar overlays por
pseudo-estado — a única outra quebra (além de `:root`) da regra "seletor =
classe":

```gss
.btn {
    background: #313244;
    text-color: #CDD6F4;
    border-radius: 8;
}
.btn:hover    { background: #45475A; }
.btn:active   { background: #1E1E2E; }
.btn:disabled { background: #181825; text-color: #6C7086; }
```

```xml
<Button class="btn" text="Enviar" onClick="enviar" />
<Button class="btn" text="Aguarde" disabled="true" />
```

- Cada bloco `:estado` sobrescreve só os campos que declara (mesma semântica
  de merge de uma classe normal), por cima da regra base — igual ao CSS.
- Resolvidos com o mesmo pipeline da regra base: `var(--x)`/tokens de
  `:root` e `@media` funcionam normalmente dentro de um bloco `:estado`.
- **Nada de rastrear hover manualmente:** cada pseudo-estado é mapeado para o
  `Status` nativo do widget do iced (`button::Status::Hovered`,
  `text_input::Status::Focused`, …), então só reage quando aquele widget
  realmente suporta o estado.
- **Cobertura atual:**
  - **`Button`** — `:hover`/`:active`/`:disabled` completos (`background`,
    `text-color`, `border-*`). Requer uma `color` base na classe (senão não
    há closure de estilo customizada para o overlay entrar). Sem overlay
    declarado, cai no comportamento histórico (±10% de luminância no
    hover/pressed; 50% de alfa quando `disabled`).
  - **`TextInput`** — `:hover`/`:focus`/`:disabled` completos, por cima do
    estilo padrão do tema (só sobrescreve os campos declarados).
  - **`Select`** — só `:hover` (borda/fundo/texto); o iced não tem um
    `Status::Disabled` para `pick_list`.
  - **`Checkbox`/`Toggle`** — só o atributo `disabled` (desliga a
    interação; usa o visual de desabilitado padrão do tema). Overlay de cor
    por pseudo-estado ainda não está implementado para esses dois.
- `disabled="true"` (ou `.gss`-independente, sempre inline) desativa o
  handler do elemento (`on_press`/`on_input`/`on_toggle`), o que também é o
  que faz `:disabled` disparar — ao contrário de `hidden`, o elemento
  continua ocupando espaço e renderizando.

Carregue uma stylesheet **global** por código:

```rust
motor.load_stylesheet("styles/app.gss")?; // vale para todos os componentes
```

…ou declare-a no template com `<link rel="stylesheet">` — também global (é
equivalente ao `load_stylesheet` acima, só que declarado no XML em vez de no
Rust). O único jeito de ter um estilo **com escopo** é um `<style scoped="true">`
inline (próxima seção). Veja `examples/estilos.rs`.

---

## Estilos inline: `<style>` / `<style scoped="true">`

Além de carregar um `.gss` externo, você pode escrever as classes **direto no
template**, num bloco `<style>`. O conteúdo é `.gss` (mesma gramática).

Por padrão esse bloco é **global** — vale em qualquer componente do app,
exatamente como `<link rel="stylesheet">` ou `motor.load_stylesheet()`, só que
sem arquivo separado. Para restringir ao componente declarante (e às classes
que só ele usa, sem risco de vazar/colidir em outro lugar), marque
`scoped="true"`:

```xml
<!-- XML: bloco GLOBAL por padrão -->
<style>
    .card  { padding: 24; background: #1E1E2E; border-radius: 16; }
    .title { size: 26; bold: true; color: #CDD6F4; }
</style>

<!-- XML: bloco COM ESCOPO, só vale na subárvore deste componente -->
<style scoped="true">
    .only_here { color: red; }
</style>

<Container class="card">
    <Text class="title" content="Olá" />
</Container>
```

- **Escopo:** sem `scoped`, o bloco entra no mesmo conjunto de sheets globais
  (`GlacierUI::stylesheets`) — qualquer componente enxerga essas classes. Com
  `scoped="true"`, as classes só valem na subárvore do componente declarante,
  **em cima** das globais — uma classe escopada de mesmo nome vence a global,
  localmente.
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
| `stylesheet` (padrão) | carrega um `.gss` | **global** | `href` |
| `import` / `component` | carrega outro template (igual a `<import>`) | global | `href`, `as`/`name` (default = nome do arquivo) |
| `data` | faz merge de um JSON no contexto | global | `href`, `as`/`name` (obrigatório) |
| `theme` | aplica uma paleta como `iced::Theme` | global (app) | `href` |

```xml
<!-- stylesheet GLOBAL: equivalente a motor.load_stylesheet() -->
<link rel="stylesheet" href="styles/estilos.gss" />

<!-- carregar um componente declarativamente -->
<link rel="import" href="templates/perfil_card.xml" as="PerfilCard" />

<!-- injetar dados de um JSON no contexto -->
<link rel="data" href="data/equipe.json" as="app" />

<!-- definir o tema da janela -->
<link rel="theme" href="styles/theme.json" />
```

- **`stylesheet`** — a sheet é sempre **global**, igual a `motor.load_stylesheet()`; não há forma escopada de um `.gss` externo. Para escopo, use um `<style scoped="true">` inline (seção anterior).
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
recompilar. A lógica Lua de um `<script>` também dispensa recompilar (é
interpretada), mas só recarrega ao re-registrar o componente.

---

## Rede e async

Um componente não fica preso à thread de UI: ele pode disparar I/O (rede, disco,
timers) por **efeitos** e receber fluxos contínuos por **subscriptions**. O
motor faz a ponte com o executor do `iced`.

**Efeitos pontuais** — dentro do `update`, chame `ctx.perform(future)`. Quando o
future completa, seus pares `(chave, valor)` são mesclados no contexto e a UI é
reavaliada:

```rust
use glacier_ui::EffectOutcome;

fn update(&mut self, action: &str, _v: Option<&str>, ctx: &mut Context) {
    if action == "carregar" {
        ctx.set("status", "carregando…");
        ctx.perform(async {
            let corpo = buscar_do_servidor().await;
            EffectOutcome::data(vec![
                ("status".into(), "ok".into()),
                ("corpo".into(), corpo),
            ])
        });
    }
}
```

**Efeito que também notifica (toast)** — o `EffectOutcome` carrega, além dos
dados, um toast opcional, e aí o motor mostra o toast do resultado — o mesmo
`ctx.show_toast` do código síncrono, só que aplicado quando o `future` resolve
(quando não há mais um `Context` vivo):

```rust
use glacier_ui::{EffectOutcome, ToastSpec};

fn update(&mut self, action: &str, _v: Option<&str>, ctx: &mut Context) {
    if action == "salvar" {
        ctx.perform(async {
            match salvar_no_servidor().await {
                Ok(_)  => EffectOutcome::data(vec![("salvo".into(), "true".into())])
                    .with_toast(ToastSpec::success("Salvo com sucesso.")),
                Err(e) => EffectOutcome::toast(ToastSpec::error(format!("Falha: {e}"))),
            }
        });
    }
}
```

Todo `ctx.perform` devolve um `EffectOutcome` (`EffectOutcome::data(...)`,
`::toast(...)` e o builder `.with_toast(...)`). Assim o efeito pede o toast pelo
caminho normal do motor, sem chaves reservadas nem interceptação no app host.

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
| `register_component(name, path)` | registra um componente de um arquivo; liga o comportamento Luau se o template tiver `<script>`, senão só a UI. |
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
    UiClick(String),                                   // on_click
    UiInputChanged { action: String, value: String },  // onChange
    Navigate(String),                                  // navigateTo
    NavigateBack,                                      // navigateBack
    FileChanged(String),                               // tick do hot-reload
    ContextPatch(Vec<(String, String)>),               // subscriptions -> contexto
    EffectOutcome(EffectOutcome),                      // efeito async: patch + toast
    UiSubmit { action: String, next_focus: Option<String> }, // Enter num formControl
    // ... DragStart/DragHover/DragEnd (drag-and-drop) e UiEditorAction (TextArea)
}
```

### Tipos de apoio

- `Template::File(String)` | `Template::Inline(String)`
- `Context` — `get`, `set`, `set_var`, `navigate_to`, `navigate_back`, `perform`, `show_toast`, `show_dialog`
- `EffectOutcome { patch, toast }` — retorno de um `ctx.perform` (dados + toast opcional); construa com `EffectOutcome::data(...)` / `::toast(...)` / `.with_toast(...)`
- `ContextVar::new(key, value)`
- `Nav::To(String)` | `Nav::Back`
- `FormBuilder::new(nome).control(FormControl::new(nome, valor_inicial)...).on_submit(closure).build()`
- `Form` — `get`/`get_mut`, `has_control`, `value`/`set_value`, `errors`/`errors_to_context`, `is_valid`, `validate`, `reset`, `control_names`, `values`, `sync_to_context`, `submit` (roda a closure de `on_submit`)
- `FormControl` — `.required()`, `.min_length(n)`, `.max_length(n)`, `.pattern(regex)`, `.validator(f)`
- `Component::on_form_submit(action, ctx)` — recebe o `onSubmit` de um `<Form>` (default: no-op); `update` nunca vê essa ação

---

## Exemplos

| Exemplo | Demonstra | Rodar |
|---|---|---|
| `contador` | `Component` básico com estado e cliques. | `cargo run --example contador` |
| `contador_macro` | comportamento embutido no template via `<script>` Luau (`register_component`). | `cargo run --example contador_macro` |
| `contador_externo` | `<script src="...">` apontando para um arquivo `.luau` externo. | `cargo run --example contador_externo` |
| `fetch_luau` | chamada de rede (`fetch`) a partir do Lua, async via corrotina. | `cargo run --example fetch_luau` |
| `imports_luau` | `require` de bibliotecas Lua (client de rede + utilitários), lógica modularizada. | `cargo run --example imports_luau` |
| `perfil` | inputs (`TextInput`), cliques, `<import>` de um cartão, `Image` e `ContextVar`. | `cargo run --example perfil` |
| `lista` | `<ForEach>` sobre JSON com um componente (`<import>`) por item. | `cargo run --example lista` |
| `condicional` | `<if>` / `<else>` (truthy e comparação). | `cargo run --example condicional` |
| `navegacao` | múltiplas telas, histórico e `navigateTo`/`navigateBack` com estado compartilhado. | `cargo run --example navegacao` |
| `aninhado` | componente registrado dentro de outro (`children()`), com roteamento por namespace. | `cargo run --example aninhado` |
| `estilos` | stylesheets `.gss` (globais e com escopo via `<link>`), classes e tema. | `cargo run --example estilos` |
| `estilos_inline` | classes `.gss` inline e com escopo via bloco `<style>` (XML). | `cargo run --example estilos_inline` |
| `formulario_login` | `Form`/`FormBuilder`/`FormControl`: validação, Enter para submeter/avançar campo. | `cargo run --example formulario_login` |

---

## Publicação no crates.io

```bash
cargo login             # token de https://crates.io/settings/tokens
cargo publish           # publica glacier-ui
```

Valide antes com `cargo publish --dry-run`.

---

## Licença

Licenciado sob **MIT OR Apache-2.0**, à sua escolha.
