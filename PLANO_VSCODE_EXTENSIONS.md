# Plano — Extensões VS Code (Glacier)

Suporte de editor para as duas linguagens do glacier-ui. Instalação **local**
apenas (publicação no Marketplace abandonada — burocracia de publisher/PAT do
Azure, ver seção final).

## Estado atual (v0.1)

### `editors/vscode/` — Glacier GSS (`.gss`)
- Realce de sintaxe específico do GSS (espelha `src/stylesheet.rs`):
  seletores `.classe` e pseudo-estados, `:root`/`var()`, `@media`, propriedades
  conhecidas vs. typos, cores hex, keywords de valor, comentários `//` e `/* */`.
- Snippets: `class`, `hover`, `root`, `media`, `var`, `card`.
- Ícone de arquivo estilo CSS.
- Verificado: `vsce package` + tokenização real (vscode-textmate) 12/12 escopos.

### `editors/vscode-gv/` — Glacier View (`.gv`)
- Realce: tags (componente vs. primitiva), atributos (ações destacadas),
  interpolação `{var|default}`, cores; **Lua embutido** em `<script>` e
  **GSS embutido** em `<style>`.
- **Go to Definition** (Ctrl/Cmd+Click, F12):
  - `on_click="fn"` → `function fn()` no `<script>` inline ou no `.luau` externo.
  - `<Componente/>` → `.gv`/`.xml` que o declara (`<import from>`, convenção de
    nome snake_case, ou `register_component("Nome","path")` no Rust).
  - Tag nativa/builtin → seção no doc de referência embutido
    (`references/glacier-view.md`).
- Verificado: lógica do provider 6/6 (harness com stub de `vscode`) + gramática
  7/7 escopos (vscode-textmate, incl. `meta.embedded.block.lua`).

## Roadmap

### Curto prazo
- [ ] **Hover** — assinatura/props do widget nativo; corpo/1ª linha da função Lua
      referenciada por uma ação.
- [ ] **Diagnóstico** — sublinhar `on_click="x"` quando não existe `function x`
      (no `<script>` inline nem no `src`); e `<import from="…">` com caminho
      inexistente.
- [ ] **Completion** — tags nativas + builtins, atributos por tag, e nomes de
      ações já definidas no `<script>`.

### Médio prazo
- [ ] **DocumentLink** visível (sublinhado) nos valores de ação e nos nomes de
      componente, além do go-to-definition.
- [ ] **Resolução de componente mais forte** — indexar `register_component`/
      `<import>` do workspace num mapa nome→arquivo, em vez de varrer a cada
      chamada; cachear e invalidar em `onDidChange`.
- [ ] **GSS**: go-to-definition de `class="card"` no `.gv` → regra `.card` no
      `.gss` linkado; e de `var(--x)` → declaração em `:root`.
- [ ] **Migração `.xml` → `.gv`** — decidir se os templates viram `.gv` (o Rust
      referencia por caminho; renomear exige atualizar os `register_component`).

### Longo prazo
- [ ] **Unificar** as duas extensões numa só "Glacier UI" (um install, um
      Makefile, um publisher) — contribui as duas linguagens + providers.
- [ ] **Formatter** (`.gv` e `.gss`).
- [ ] **Preview** ao vivo da tela (reaproveitar o hot-reload do motor).

## Instalação (local)

Cada extensão tem um `Makefile`:

```bash
# Glacier View (.gv)
cd editors/vscode-gv && make install

# Glacier GSS (.gss)
cd editors/vscode && make install
```

`make reinstall` após editar gramática/JS; `make uninstall` para remover.
Requer `code` no PATH e `npx` (Node).

## Nota sobre publicação no Marketplace

Bloqueada do lado da Microsoft: criação de publisher retornou
"Publisher Metadata has suspicious content" e depois rate limit
`Count/VSID` (conta nova). Decisão: **ficar em instalação local** via `.vsix`.
Se retomar: `vsce login <publisher>` + `vsce publish` (precisa de publisher
registrado e PAT com escopo *Marketplace → Manage*).
