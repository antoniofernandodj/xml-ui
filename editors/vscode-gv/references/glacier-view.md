# Glacier View (`.gv`) — referência de sintaxe

Markup XML do glacier-ui. Uma tela é uma árvore de tags; atributos configuram cada
widget; `{var}` interpola o contexto; `<script>` carrega o comportamento em Lua;
`<style>` carrega GSS.

## Estrutura

```gv
<Container padding="24" background="#2E3440">
  <Column spacing="16" align="Center">
    <Text content="Olá {nome}" size="22" />
    <Button text="Clique" on_click="fazer_algo" />
  </Column>
</Container>

<script>
function fazer_algo()
  ctx.nome = "mundo"
end
</script>
```

- **Interpolação**: `content="Valor: {contador}"` — lê `ctx.contador`. `{k|default}` usa `default` quando a chave falta.
- **Ações**: `on_click`, `onClick`, `on_change`/`onChange`, `on_toggle`, `on_submit`, `on_reorder`, `on_open`/`on_message`/`on_error`/`on_close`, mais as variantes `ao_*`/`aoX`. O valor é o **nome de uma função** definida no `<script>` (inline ou `src=`).
- **Comportamento**: `<script>…</script>` (Lua inline) ou `<script src="arquivo.luau"></script>` (externo, relativo ao template).
- **Estilo**: `<style>…</style>` (GSS inline) ou `<link rel="stylesheet" href="app.gss"/>`. Ver a extensão *Glacier GSS*.

---

## Widgets primitivos

### `<Container>`
Caixa única com padding/fundo/borda. Aceita um filho. Atributos: `padding`, `background`, `width`, `height`, `alignX`, `alignY`, `border-radius`, `border-width`, `border-color`.

### `<Column>`
Empilha filhos na vertical. Atributos: `spacing`, `align` (eixo cruzado = X), `width`, `height`.

### `<Row>`
Empilha filhos na horizontal. Atributos: `spacing`, `align` (eixo cruzado = Y).

### `<Text>` (`<Span>`)
Texto. Conteúdo via `content="…"` ou filho `<Text>…</Text>`. Atributos: `size`, `bold`, `color`.

### `<Button>` (`<Botao>`)
Botão. Atributos: `text`, `on_click`, `navigateTo`, `navigateBack`, `color`.

### `<TextInput>` (`<Input>`)
Campo de texto de uma linha. Atributos: `value`, `placeholder`, `onChange`, `secure`, `formControl`.

### `<TextArea>` (`<Editor>`)
Editor multilinha. Atributos: `value`, `placeholder`, `onChange`.

### `<Image>` (`<Imagem>`)
Imagem. Atributos: `source`/`src`, `clip="Circle"`.

### `<Svg>` (`<Icon>`)
Ícone/SVG. Atributos: `source`/`src`, `color`.

### `<Scrollable>` (`<Scroll>`)
Área rolável. Atributos: `direction` (`vertical`/`horizontal`).

### `<Checkbox>` (`<Check>`)
Caixa de seleção. Atributos: `label`, `checked`, `onToggle`.

### `<Toggle>` (`<Switch>`)
Interruptor. Atributos: `label`, `checked`, `onToggle`.

### `<Rule>` (`<Divider>`)
Divisória. Atributos: `direction`.

### `<Select>` (`<Dropdown>`, `<ComboBox>`)
Seletor. Atributos: `options`, `value`, `onChange`, `placeholder`, `labelField`, `valueField`, `color`.

### `<Form>` (`<Formulario>`)
Formulário. Atributos: `onSubmit`, `name`. Envolve `formControl`s.

---

## Controle de fluxo e composição

### `<ForEach>` (`<For>`)
Repete o corpo por item. Atributos: `items`, `var`.

### `<If>` (`<Se>`) / `<Else>` (`<Senao>`)
Condicional. `<If>` aceita `cond`, `equals`, `notEquals`.

### `<Include>` (`<Incluir>`)
Inclui outro template. Atributo: `src`; demais atributos viram props.

### `<Import>` (`<Importar>`)
Registra um componente por nome. Atributos: `name`/`as`, `from`.

---

## Builtins (registrados pela lib)

### `<Badge>`
Rótulo/etiqueta embutido de `src/builtins.rs`. Ver `BUILTINS.md` para estender.

---

Componentes do **app** são qualquer tag desconhecida (ex.: `<PerfilCard/>`),
resolvida pelo nome — declarada em outro `.gv`, por `<import from>`, ou por
`register_component("Nome", "caminho")` no Rust.
