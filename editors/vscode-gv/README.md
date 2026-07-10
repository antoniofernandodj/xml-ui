# Glacier View — VS Code extension

Language support for **Glacier View** (`.gv`), the XML markup of `glacier-ui`.

## Features (v1)

- **Syntax highlighting** for Glacier View markup:
  - Tags (primitives, builtins, and app components), attributes, `{interpolation}`
    with `{key|default}`, and hex colors.
  - Action attributes (`on_click`, `onChange`, `on_toggle`, …) are highlighted
    distinctly, and their handler names are scoped as functions.
  - **Embedded Lua** inside `<script>…</script>`.
  - **Embedded GSS** inside `<style>…</style>` (needs the *Glacier GSS* extension
    for full GSS colors).
- **Go to Definition** (Ctrl/Cmd+Click, F12):
  - `on_click="foo"` → Lua `function foo()` in the inline `<script>` block, or in
    the external file of `<script src="foo.luau">`.
  - `<PerfilCard/>` → the `.gv`/`.xml` that declares it (via `<import from>`,
    filename convention, or `register_component("Name", "path")` in Rust).
  - `<Button/>`, `<Badge/>`, … (native/builtin) → the bundled syntax reference.

Starts simple, meant to grow (hovers, unknown-handler diagnostics, completion).

## Install (local)

```bash
cd editors/vscode-gv
make install     # packages the .vsix and installs it into VS Code
```

Then open any `.gv` file. To try it on the existing `.xml` templates, either
rename them to `.gv` or right-click → *Change Language Mode* → *Glacier View*.
