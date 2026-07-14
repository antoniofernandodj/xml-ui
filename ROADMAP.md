# Roadmap — rumo a um `1.0` publicável

Roteiro para levar o `glacier-ui` de protótipo enxuto a um framework maduro.

> **Nota (0.38.0).** As versões anteriores deste arquivo descreviam a lib de
> muito tempo atrás ("~1.3K LOC, 7 widgets, 11 testes") e faziam parecer pendente
> um monte de coisa que já existe. A tabela abaixo é o estado real; o histórico
> foi reescrito para não voltar a enganar.

## Onde estamos hoje (0.38.0)

| Indicador | Estado |
|---|---|
| Código | ~9K LOC em `src`, ~210 testes |
| Erros | ✅ `GlacierError` tipado + `Diagnostic` (arquivo:linha:coluna, trecho, dica) |
| Encapsulamento | ✅ campos do `GlacierUI` privados, com getters; erro `#[non_exhaustive]` |
| Widgets | 15 (`container column row text button text_input textarea image svg scrollable checkbox toggle rule select form`) + diretivas (`if/else`, `ForEach`, `import`, `include`, `fragment`) |
| Estilo | `.gss` com `:root`/`var()`, pseudo-estados, `@media`, seletores de classe/id/tag e **lista por vírgula** |
| Reavaliação | ✅ escopada: só a tela ativa (+ fixados) é construída, não todo template registrado |
| Runner | `GlacierDaemon` multi-janela: fontes, `window::Settings`, janelas-filhas, `on_message`, `on_close` |
| Distribuição | publicado no crates.io; LICENSE + metadata OK. **Falta CI e CHANGELOG.** |

---

## Feito

- **Fase 0** — LICENSE, metadata, publicação no crates.io.
- **Fase 1 (robustez)** — erro tipado (`error.rs`), encapsulamento do `GlacierUI`,
  diagnóstico posicional em XML e `.gss`.
- **Fase 2 (completude)** — cobertura de widgets, design tokens/temas,
  `if`/`else`/`ForEach`, componentes, multi-janela.
- **Dirty-tracking** — `reevaluate_all` deixou de reconstruir a árvore uma vez
  por template registrado (ver o doc do método).

## Pendente

### Qualidade / infra
- [ ] **CI** (GitHub Actions): `build` + `test` + `clippy -D warnings` +
      `fmt --check` + `cargo doc`.
- [ ] **`CHANGELOG.md`** (keep-a-changelog) + `cargo-release`/`release-plz`.
- [ ] **`#![deny(missing_docs)]`** no crate root.
- [ ] **`cargo-deny`** — licenças e advisories.
- [ ] **Benchmarks (`criterion`)** — medir o custo de avaliação por tamanho de
      árvore, agora que a reavaliação é escopada.
- [ ] **Testes de render** — snapshot da árvore avaliada; fuzzing leve no parser.

### Motor
- [ ] **Seletores compostos/descendentes** no `.gss` + especificidade + `!important`
      (item 9 do `PLANO_GSS_LIMITACOES.md`).
- [ ] **Propriedades extras** sob demanda: `opacity`, `shadow`, borda por lado,
      gradiente radial, `transform` (item 10).
- [ ] **Render sem panic** — degradar markup malformado para um nó de erro
      visível em vez de derrubar o `view`.
- [ ] **Estado por instância em `ForEach`** — hoje N instâncias de um componente
      com estado dentro de uma lista compartilham o mesmo `update`.
- [ ] **Linguagem de template mais rica** — expressões, `else if`, eventos com
      argumentos.

### Apresentação
- [ ] **Guia (`mdBook`)** — tutorial + referência da DSL.
- [ ] **Galeria de exemplos com screenshots/GIFs.**
- [ ] Congelar a API → `1.0`.
