# Roadmap — rumo a um `1.0` publicável

Roteiro para levar o `xml-ui` de protótipo enxuto a um framework maduro e
distribuível publicamente no crates.io.

## Onde estamos hoje

| Indicador | Estado |
|---|---|
| Código | ~1.3K LOC em `src`, 140 na macro, 509 de testes |
| Testes | 11, todos num arquivo; 0 na macro, 0 de render, 0 doctests |
| Erros | 12 assinaturas `Result<_, String>` — sem tipo de erro |
| Widgets | 7 (`button, column, row, text, container, text_input, image`) |
| Docs | 0 `//!` no crate root; sem `deny(missing_docs)` |
| Distribuição | sem LICENSE, CI, CHANGELOG; sem metadata de publicação |

Pontos fortes: quase nenhum `unwrap`/`panic` no runtime, zero TODO/FIXME,
arquitetura limpa. O gargalo de maturidade **não é o tamanho** — é
infraestrutura (erro tipado, encapsulamento, completude de widgets, docs e
distribuição).

---

## Fase 0 — Bloqueadores de publicação

Barato e de alto valor; viável em ~1 dia.

- [ ] **LICENSE** dual `MIT OR Apache-2.0` (convenção Rust).
- [ ] **Metadata no `Cargo.toml`** dos dois crates: `description`, `license`,
      `repository`, `keywords`, `categories`, `readme`, `rust-version` (MSRV).
      A macro deve publicar junto, com versão casada (`=x.y.z`).
- [ ] **Nome único no crates.io** — conferir disponibilidade de `xml-ui`;
      considerar rebatizar para algo de marca.
- [ ] **CI** (GitHub Actions): `build` + `test` + `clippy -D warnings` +
      `fmt --check` + `cargo doc`. Zerar os ~10 warnings de clippy da lib.
- [ ] **`#![deny(missing_docs)]`** + doc no crate root (`//!`).

## Fase 1 — Robustez de biblioteca

- [ ] **Tipo de erro real** — substituir os 12 `Result<_, String>` por um
      `enum XmlUiError` com `thiserror` (arquivo ausente, XML inválido,
      componente não registrado, …). **Maior alavancagem.**
- [ ] **Encapsular o `UiEngine`** — campos hoje são todos `pub`; torná-los
      privados (+ getters) e marcar enums públicos com `#[non_exhaustive]`.
- [ ] **Política de semver** — crates em lockstep; documentar superfície estável.
- [ ] **Render sem panic** — degradar input malformado para um nó de erro
      visível, nunca panicar em `widget.rs`/`dispatch`.

## Fase 2 — Completude funcional

- [ ] **Cobertura de widgets** — prioridade `scrollable`; depois `checkbox`,
      `toggler`, `slider`, `pick_list`/dropdown, `radio`, `space`, `rule`,
      `progress_bar`, `tooltip`, `svg`.
- [ ] **Temas/estilo reutilizável** — tokens de tema / "classes" / herança de
      estilo em vez de hex inline repetido; integrar com o tema do `iced`.
- [ ] **Linguagem de template mais rica** — expressões simples, negação no
      `if`, `else if`, eventos com argumentos.
- [ ] **Estado por instância em `ForEach`** — IDs de instância para componentes
      com estado dentro de listas (hoje N instâncias compartilham um `update`).
- [ ] **Contexto tipado (opcional)** — avaliar valores tipados/serializáveis
      para reduzir `to_string()`/parse manual.

## Fase 3 — Qualidade e DX

- [ ] **Testes de macro** — `trybuild` para casos compile-fail e pass.
- [ ] **Testes de render** — snapshot da árvore avaliada; property tests no
      `parser`/`process_template`; fuzzing leve no parser.
- [ ] **Benchmarks (`criterion`)** + **dirty-tracking** — hoje `reevaluate_all`
      reavalia todos os templates e clona o contexto a cada mudança (O(n)
      cheio); reavaliar só o que mudou.
- [ ] **`cargo-deny`** — auditoria de licenças e advisories no CI.

## Fase 4 — Apresentação e lançamento

- [ ] **Guia/livro (`mdBook`)** — tutorial + referência da DSL.
- [ ] **Galeria de exemplos com screenshots/GIFs.**
- [ ] **`CHANGELOG.md`** (keep-a-changelog) + automação (`cargo-release`/`release-plz`).
- [ ] **Anúncio** — `1.0.0` no crates.io, post no fórum do `iced` / `r/rust`, docs.rs.

---

## Caminho crítico

```
Fase 0 (1 dia)  →  publicável "early/0.x"
  + erro tipado + encapsulamento        →  confiável como dependência
  + scrollable + temas                  →  usável em app real
  + dirty-tracking + testes macro/render →  defensável como "maduro"
  → congela API → 1.0
```

A base está arquiteturalmente pronta para crescer; o trabalho é incremental,
não refatoração de fundação.
