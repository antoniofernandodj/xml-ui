# Plano: Suporte a Templates KDL no glacier-ui

## Motivação

XML é verboso. Tags de fechamento, aspas obrigatórias em todo atributo e o
ruído de `<link rel="...">` tornam os templates mais longos do que precisam
ser. O [KDL](https://kdl.dev) (Kdl Document Language) oferece uma alternativa
limpa com hierarquia por chaves, argumentos posicionais e propriedades
nomeadas — tudo com parser Rust já disponível no crates.io (`kdl` crate).

### Comparativo

**XML (atual):**
```xml
<link rel="theme"      href="styles/theme.json" />
<link rel="stylesheet" href="styles/estilos.iss" />
<import name="PerfilCard" from="templates/perfil_card.xml" />

<Container class="card">
    <Column class="stack">
        <Text class="title" content="Estilos via .iss" />
        <Text class="subtitle" content="Contador: {valor}" />
        <Row class="actions">
            <Button class="btn btn-danger" text="-" onClick="decrementar" />
            <Button class="btn btn-success" text="+" onClick="incrementar" />
        </Row>
    </Column>
</Container>
```

**KDL (proposto):**
```kdl
theme  "styles/theme.json"
style  "styles/estilos.iss"
import "PerfilCard" from="templates/perfil_card.xml"

Container class="card" {
    Column class="stack" {
        Text "Estilos via .iss" class="title"
        Text "Contador: {valor}" class="subtitle"
        Row class="actions" {
            Button "-" onClick="decrementar" class="btn btn-danger"
            Button "+" onClick="incrementar" class="btn btn-success"
        }
    }
}
```

---

## Sintaxe do formato `.kdl`

### Nós de declaração (topo do arquivo)

| Nó KDL | Equivalente XML | Descrição |
|--------|----------------|-----------|
| `theme "path.json"` | `<link rel="theme" href="...">` | Paleta global do iced |
| `style "path.iss"` | `<link rel="stylesheet" href="...">` | Stylesheet escopada ao componente |
| `import "Nome" from="path.xml"` | `<import name="..." from="...">` | Importa componente externo |
| `data "path.json" as="chave"` | `<link rel="data" href="..." as="...">` | JSON injetado no contexto |

### Nós de layout (árvore de UI)

```kdl
// Conteúdo de texto/botão como primeiro argumento posicional
Text "Olá, {nome}!" size=28 bold=true color="#ECEFF4"
Button "Clique" onClick="acao" color="#A3BE8C"

// Propriedades booleanas abreviadas: bold=true → apenas bold
Text "Título" bold size=32

// Filhos dentro de chaves
Column spacing=20 {
    Row spacing=10 {
        ...
    }
}

// Sem filhos: sem chaves (folha)
Text "Simples" size=16
```

### Regras de atributos

- Strings com espaço ou caracteres especiais exigem aspas: `padding="10 20"`
- Cores hexadecimais sem espaço não precisam de aspas: `color=#2E3440` (o parser
  trata como string)
- Valores numéricos sem aspas: `size=28`, `spacing=15`
- Strings simples sem espaço sem aspas: `align=Center`, `width=fill`

### Controle de fluxo

```kdl
// Condicional — atributo if
Column if="{logado}" {
    Text "Bem-vindo!"
}
Column else=true {
    Text "Faça login"
}

// Loop — atributo for-each
CartaoUsuario for-each="usuarios" var="u" nome="{u.nome}" cargo="{u.cargo}"
```

### Script (componentes com lógica embutida)

O bloco `<script>` continua suportado via um nó especial no final do arquivo:

```kdl
Container { ... }

script {
    // Rust puro — mesmas regras do <script> em XML
    fn incrementar(self) {
        self.contador += 1;
    }
}
```

---

## Plano de implementação

### Fase 1 — Dependência e parser base

**1.1** Adicionar o crate `kdl` ao `Cargo.toml`:
```toml
kdl = "6"
```

**1.2** Criar `src/kdl_parser.rs` com:
- `pub fn parse_kdl(input: &str) -> Result<UiNode, String>` — ponto de entrada
- `fn node_from_kdl(node: &kdl::KdlNode) -> Option<UiNode>` — converte um nó KDL
  em `UiNode`, reutilizando os mesmos `NodeType` e `UiNode` do parser XML
- Helpers para extrair o primeiro argumento posicional (conteúdo de `Text`,
  `Button`, etc.) e as propriedades nomeadas como atributos
- Separação entre nós de declaração (`theme`, `style`, `import`, `data`) e nós
  de layout (qualquer outro nome de nó)

**1.3** Extrair o bloco `script` antes de passar para o parser KDL (equivalente
ao `strip_script` do XML, mas procurando o nó `script { }` no topo do documento).

### Fase 2 — Integração no engine

**2.1** Em `src/lib.rs`, atualizar a função que lê templates de arquivo para
detectar a extensão:
- `.xml` → caminho atual (`UiNode::parse_xml`)
- `.kdl` → novo `parse_kdl`

**2.2** Expor `parse_kdl` no `pub use` de `lib.rs`.

**2.3** O `Template::File` já recebe um caminho como string — nenhuma mudança
na API pública.

### Fase 3 — Exemplos e templates

**3.1** Criar `templates/contador.kdl` — versão KDL do contador existente,
para validar o parser.

**3.2** Criar `examples/contador_kdl.rs` — exemplo mínimo usando o novo formato.

**3.3** Criar `templates/estilos.kdl` — valida `theme`, `style` e classes.

### Fase 4 — Cobertura de testes

**4.1** Em `tests/engine_tests.rs`, adicionar casos cobrindo:
- Parsing básico de nós de layout
- Nós de declaração (`theme`, `style`, `import`, `data`)
- Argumento posicional como conteúdo (`Text "..."`, `Button "..."`)
- Atributos booleanos (`bold=true`)
- Controle de fluxo (`if`, `else`, `for-each`)
- Arquivo `.kdl` rodando end-to-end no engine

---

## Não está no escopo

- Migração automática de templates XML existentes para KDL
- Deprecação do suporte XML (os dois formatos coexistem indefinidamente)
- Novo formato de stylesheet `.iss` — o KDL é só para templates de UI
- Ferramenta de formatação/linting de `.kdl` (o `kdl` crate já tem `fmt`)

---

## Referências

- Spec KDL: https://kdl.dev
- Crate `kdl` (v6): https://crates.io/crates/kdl
- Parser XML atual: `src/parser.rs`
- Engine: `src/lib.rs`
