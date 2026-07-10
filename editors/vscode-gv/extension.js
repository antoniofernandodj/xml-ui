// Glacier View (.gv) language support.
//
// v1 provides Go-to-Definition (Ctrl/Cmd+Click, F12) for:
//   1. action attributes  on_click="foo"  ->  Lua `function foo()` in the inline
//      <script> block, or in the external file of <script src="foo.luau">.
//   2. component tags  <PerfilCard/>  ->  the .gv/.xml that declares it (via
//      <import from>, filename convention, or register_component in Rust).
//   3. native tags  <Button/>, <Badge/>  ->  the bundled syntax reference.
//
// Intentionally simple and dependency-free (plain JS, no build step). Meant to
// grow: hovers, diagnostics for unknown handlers, completion, etc.

const vscode = require("vscode");
const fs = require("fs");
const path = require("path");

// Canonical native tag -> all recognised spellings (from src/parser.rs). Used to
// tell a native/builtin widget apart from an app component, and to anchor the
// native tag into the bundled reference doc.
const NATIVE_TAGS = {
  Container: ["container"],
  Column: ["column"],
  Row: ["row"],
  Text: ["text", "span"],
  Button: ["button", "botao"],
  TextInput: ["textinput", "input", "entrada_texto"],
  TextArea: ["textarea", "texteditor", "editor", "area_texto"],
  Image: ["image", "imagem"],
  Svg: ["svg", "icon", "icone"],
  Scrollable: ["scrollable", "scroll", "rolagem"],
  Checkbox: ["checkbox", "check"],
  Toggle: ["toggle", "toggler", "switch"],
  Rule: ["rule", "divider", "divisoria"],
  Select: ["select", "dropdown", "picklist", "combobox", "combo", "seletor"],
  Form: ["form", "formulario"],
  Include: ["include", "incluir"],
  Import: ["import", "importar"],
  ForEach: ["foreach", "for"],
  If: ["if", "se"],
  Else: ["else", "senao"],
  Badge: ["badge"],
};

// Lowercased spelling -> canonical name.
const NATIVE_LOOKUP = {};
for (const [canon, variants] of Object.entries(NATIVE_TAGS)) {
  NATIVE_LOOKUP[canon.toLowerCase()] = canon;
  for (const v of variants) NATIVE_LOOKUP[v.toLowerCase()] = canon;
}

// An action attribute whose value is a handler function name.
const ACTION_ATTR_RE = /^(on[_-]?[A-Za-z]+|ao[A-Z][A-Za-z]*|ao_[a-z_]+)$/;

/** Find `function <name>(` inside `text`; return the char offset of `name` or -1. */
function findLuaFunctionOffset(text, name) {
  const re = new RegExp(
    "function\\s+(" + escapeRe(name) + ")\\s*\\(", "g"
  );
  const m = re.exec(text);
  if (m) return m.index + m[0].indexOf(m[1]);
  // Also `foo = function(...)` style.
  const re2 = new RegExp(
    "(?:^|[^\\w.])(" + escapeRe(name) + ")\\s*=\\s*function\\b", "g"
  );
  const m2 = re2.exec(text);
  if (m2) return m2.index + m2[0].indexOf(m2[1]);
  return -1;
}

function escapeRe(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Convert a char offset in `text` to a vscode.Position. */
function offsetToPosition(text, offset) {
  let line = 0;
  let last = 0;
  for (let i = 0; i < offset; i++) {
    if (text.charCodeAt(i) === 10) {
      line++;
      last = i + 1;
    }
  }
  return new vscode.Position(line, offset - last);
}

/** Locations for a Lua handler `name`, searching inline + external <script>. */
function resolveHandler(document, name) {
  const full = document.getText();
  const out = [];

  // Inline <script> ... </script> (skip ones with src=).
  const inlineRe = /<script(?![^>]*\bsrc\s*=)[^>]*>([\s\S]*?)<\/script>/gi;
  let m;
  while ((m = inlineRe.exec(full)) !== null) {
    const body = m[1];
    const bodyStart = m.index + m[0].indexOf(body);
    const off = findLuaFunctionOffset(body, name);
    if (off >= 0) {
      out.push(new vscode.Location(document.uri, offsetToPosition(full, bodyStart + off)));
    }
  }

  // External <script src="foo.luau"> resolved relative to the document.
  const srcRe = /<script[^>]*\bsrc\s*=\s*"([^"]+)"[^>]*>/gi;
  const baseDir = path.dirname(document.uri.fsPath);
  while ((m = srcRe.exec(full)) !== null) {
    const p = path.resolve(baseDir, m[1]);
    try {
      const text = fs.readFileSync(p, "utf8");
      const off = findLuaFunctionOffset(text, name);
      if (off >= 0) {
        out.push(new vscode.Location(vscode.Uri.file(p), offsetToPosition(text, off)));
      }
    } catch (_) {
      // missing/unreadable external script — ignore
    }
  }
  return out;
}

/** Resolve a component tag to its declaration file(s). */
async function resolveComponent(document, tag) {
  const full = document.getText();
  const baseDir = path.dirname(document.uri.fsPath);
  const out = [];

  // 1. Explicit <import name="Tag" from="path"/> (also `as=`).
  const impRe = new RegExp(
    "<import\\b[^>]*\\b(?:name|nome|as)\\s*=\\s*\"" + escapeRe(tag) +
    "\"[^>]*\\b(?:from|de|src|path|caminho)\\s*=\\s*\"([^\"]+)\"", "i"
  );
  const im = impRe.exec(full);
  if (im) {
    const p = path.resolve(baseDir, im[1]);
    if (fs.existsSync(p)) out.push(new vscode.Location(vscode.Uri.file(p), new vscode.Position(0, 0)));
  }
  if (out.length) return out;

  // 2. Filename convention: Tag.gv / snake_case(Tag).gv (+ .xml legacy).
  const snake = tag.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();
  const candidates = new Set([tag, snake]);
  for (const base of candidates) {
    for (const ext of [".gv", ".xml"]) {
      const found = await vscode.workspace.findFiles("**/" + base + ext, "**/target/**", 5);
      for (const uri of found) out.push(new vscode.Location(uri, new vscode.Position(0, 0)));
    }
  }
  if (out.length) return out;

  // 3. register_component("Tag" | "snake", "path") anywhere in the workspace.
  const regFiles = await vscode.workspace.findFiles("**/*.{rs,lua,luau}", "**/target/**", 200);
  const nameAlt = tag + "|" + escapeRe(snake);
  const regRe = new RegExp(
    "register_component\\s*\\(\\s*\"(?:" + nameAlt + ")\"\\s*,\\s*\"([^\"]+)\"", "i"
  );
  for (const uri of regFiles) {
    let text;
    try { text = fs.readFileSync(uri.fsPath, "utf8"); } catch (_) { continue; }
    const rm = regRe.exec(text);
    if (rm) {
      const p = path.resolve(vscode.workspace.getWorkspaceFolder(uri).uri.fsPath, rm[1]);
      const target = fs.existsSync(p) ? vscode.Uri.file(p) : uri;
      out.push(new vscode.Location(target, new vscode.Position(0, 0)));
    }
  }
  return out;
}

/** Jump a native tag to its heading in the bundled reference doc. */
function resolveNative(context, canonical) {
  const ref = path.join(context.extensionPath, "references", "glacier-view.md");
  let text = "";
  try { text = fs.readFileSync(ref, "utf8"); } catch (_) { return []; }
  const re = new RegExp("^#+\\s*`?<?" + escapeRe(canonical) + "\\b", "mi");
  const m = re.exec(text);
  const pos = m ? offsetToPosition(text, m.index) : new vscode.Position(0, 0);
  return [new vscode.Location(vscode.Uri.file(ref), pos)];
}

/**
 * Classify what the cursor is on. Returns {kind:'action', name} for an action
 * attribute value, {kind:'tag', name} for a tag name, or null.
 */
function classify(document, position) {
  const line = document.lineAt(position.line).text;
  const ch = position.character;

  // Attribute value: name = "value" with cursor inside the quotes.
  const attrRe = /([A-Za-z_][\w.-]*)\s*=\s*"([^"]*)"/g;
  let a;
  while ((a = attrRe.exec(line)) !== null) {
    const valStart = a.index + a[0].lastIndexOf('"', a[0].length - 2) + 1;
    const valEnd = a.index + a[0].length - 1;
    if (ch >= valStart && ch <= valEnd && ACTION_ATTR_RE.test(a[1])) {
      const val = a[2].trim();
      if (val) return { kind: "action", name: val };
    }
  }

  // Tag name: <Name  or  </Name
  const wordRange = document.getWordRangeAtPosition(position, /[A-Za-z_][\w.-]*/);
  if (wordRange) {
    const before = line.slice(0, wordRange.start.character);
    if (/<\/?\s*$/.test(before)) {
      const name = line.slice(wordRange.start.character, wordRange.end.character);
      return { kind: "tag", name };
    }
  }
  return null;
}

function activate(context) {
  const provider = {
    async provideDefinition(document, position) {
      const hit = classify(document, position);
      if (!hit) return undefined;

      if (hit.kind === "action") {
        return resolveHandler(document, hit.name);
      }

      // tag
      const canonical = NATIVE_LOOKUP[hit.name.toLowerCase()];
      if (canonical) {
        return resolveNative(context, canonical);
      }
      return resolveComponent(document, hit.name);
    },
  };

  context.subscriptions.push(
    vscode.languages.registerDefinitionProvider({ language: "glacier-view" }, provider)
  );
}

function deactivate() {}

module.exports = { activate, deactivate };
