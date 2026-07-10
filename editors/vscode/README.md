# Glacier GSS — VS Code extension

Language support for **Glacier Stylesheets** (`.gss`), the CSS-like styling
format used by `glacier-ui`.

## Features

- **Syntax highlighting** tuned to the GSS grammar:
  - Class selectors (`.card`), id selectors (`#save`, higher specificity than
    a class), and pseudo-states (`:hover`, `:focus`, `:active`, `:pressed`,
    `:disabled`) on either — unknown pseudo-states are flagged.
  - `:root { --token: value; }` design tokens and `var(--token, fallback)`
    references.
  - `@media (min-width: 600) and (max-width: 900) { … }` responsive blocks.
  - Known properties are highlighted as such; typos (unknown properties) are
    marked so they stand out — matching the engine, which ignores them.
  - Hex colors (`#RRGGBB`/`#RRGGBBAA`), numbers with optional `px`, and value
    keywords (`fill`, `Center`, `none`, `true`, …).
  - `//` line comments and `/* … */` block comments.
- **Snippets**: `class`, `id`, `hover`, `root`, `media`, `var`, `card`.
- **A CSS-style file icon** so `.gss` files read as stylesheets in the explorer.

## Install (local / development)

```bash
cd editors/vscode
npm install -g @vscode/vsce   # once
vsce package                  # produces glacier-gss-0.1.0.vsix
code --install-extension glacier-gss-0.1.0.vsix
```

Or press **F5** in VS Code with this folder open to launch an Extension
Development Host.

## Grammar reference

The highlighting mirrors `src/stylesheet.rs`. Supported property names include
their `kebab`, `snake`, and `camel` spellings (e.g. `align-x` / `align_x` /
`alignX`) plus the `w`/`h`/`bg` shorthands.
