# Plano: Iced Stylesheet (`.iss`)

## Problema atual

Todos os atributos de estilo estão embutidos no XML inline:

```xml
<Container background="#2E3440" border-radius="12" padding="16" align-x="center">
```

Isso causa repetição, dificulta manutenção e mistura layout com aparência.

---

## Formato `.iss` proposto

Sintaxe CSS-like, simples de parsear e familiar:

```iss
// styles/app.iss

.card {
    background: #2E3440;
    border-radius: 12;
    border-width: 1;
    border-color: #4C566A;
    padding: 16;
}

.centered {
    align-x: center;
    align-y: center;
}

.btn-primary {
    color: #A3BE8C;
    padding: 8 16;
}

.full-width {
    width: fill;
}
```

Uso no XML via atributo `class`:

```xml
<Container class="card centered">
    <Button class="btn-primary full-width" text="Enviar" on:click="submit" />
</Container>
```

Atributos inline **sobrescrevem** classes (mesma precedência do CSS):

```xml
<Container class="card" padding="32">  <!-- padding=32 vence o .card -->
```

---

## Estrutura de dados (`src/stylesheet.rs` — novo módulo)

```rust
// Espelha os campos de estilo de UiNode
pub struct StyleRule {
    pub width: Option<String>,
    pub height: Option<String>,
    pub padding: Option<String>,
    pub spacing: Option<f32>,
    pub align_x: Option<String>,
    pub align_y: Option<String>,
    pub background: Option<String>,
    pub border_radius: Option<f32>,
    pub border_width: Option<f32>,
    pub border_color: Option<String>,
    pub color: Option<String>,
    pub size: Option<f32>,
    pub bold: Option<bool>,
}

pub struct StyleSheet {
    pub rules: HashMap<String, StyleRule>,  // ".card" -> StyleRule
}
```

---

## Alterações nos arquivos existentes

| Arquivo | Mudança |
|---|---|
| `src/parser.rs` | Adicionar `class: Option<String>` em `UiNode`; parsear atributo `class` |
| `src/widget.rs` | Receber `&StyleSheet`; resolver classes antes de aplicar attrs inline; mesclagem `class < inline` |
| `src/lib.rs` | Adicionar `stylesheets: Vec<StyleSheet>` em `UiEngine`; método `load_stylesheet(path)` |
| `src/eval.rs` | Passar stylesheet na pipeline de avaliação |
| `src/stylesheet.rs` | Novo: parser `.iss` + structs |

---

## Algoritmo de mesclagem de estilos (em `widget.rs`)

```
1. Iniciar com StyleRule vazio
2. Para cada classe em class="a b c" (esquerda para direita):
   → aplicar campos non-None da classe (sobrescreve anterior)
3. Para cada atributo inline no UiNode:
   → sobrescrever o campo correspondente
4. Resultado final é o conjunto de estilos aplicados ao widget
```

---

## API pública proposta

```rust
// Carregamento de arquivo
engine.load_stylesheet("styles/app.iss");

// Ou inline no XML com tag especial (fase futura)
// <link stylesheet="styles/app.iss" />
```

---

## Fases de implementação

### Fase 1 — Parser `.iss`
- Criar `src/stylesheet.rs` com `parse_iss()` e structs
- Suporte a: seletores `.nome`, propriedades `chave: valor;`, comentários `//`

### Fase 2 — Integração com `UiNode`
- Adicionar `class` em `UiNode` e parsear no `parser.rs`
- Passar `StyleSheet` pela pipeline de renderização
- Implementar mesclagem em `widget.rs`

### Fase 3 — UX e hot-reload
- Múltiplos arquivos `.iss` carregados por ordem de prioridade
- Observar mudanças em `.iss` no modo hot-reload (já existe o watcher em `lib.rs`)
- Erros de classe inválida com mensagem clara (classe `"foo"` não encontrada)

---

## Fora do escopo inicial

- Herança de classe (`.card.dark` herdando de `.card`)
- Seletores por tipo de widget (`Button.primary`)
- Variáveis/tokens (`$cor-primaria: #A3BE8C`)
- Pseudo-estados (`:hover`, `:pressed`) — estes ficam no Iced
