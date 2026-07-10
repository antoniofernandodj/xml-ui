# Plano de construção da biblioteca de widgets — rumo a um "Qt em Rust"

Este documento é o **planejamento de longo prazo** da biblioteca de widgets do
`glacier-ui`. A meta declarada é concorrer diretamente com o **Qt**: acumular,
ao longo dos anos, um catálogo vasto de widgets — de `Button` a `QDateTimeEdit`,
de diálogos de arquivo a árvores model/view — todos como **componentes Rust** que
carregam **estrutura** (template), **estilo** (`.gss`) e **comportamento**
(`update` + Luau).

É um documento vivo. Cada linha da tabela é um item de backlog; conforme um
widget nasce, seu status vira ✅ e ele ganha exemplo em `examples/` e doc curta.

Relacionados: [`BUILTINS.md`](BUILTINS.md) (como escrever um builtin),
[`DIALOGS.md`](DIALOGS.md) (diálogos modais), [`ROADMAP.md`](ROADMAP.md)
(maturidade do motor).

---

## 1. Como um widget Qt vira um componente glacier-ui

O glacier-ui já tem três níveis (ver `BUILTINS.md`). Cada widget Qt do catálogo
abaixo é classificado em um deles, mais dois auxiliares:

| Nível | O que é | Onde vive | Analogia Qt |
|---|---|---|---|
| **Primitiva** | Nó nativo do motor, mapeado 1:1 a um widget do `iced` | `widget.rs` + `parser.rs` | folha atômica (`QPushButton`) |
| **Builtin** | `impl Component` que a lib auto-registra; template inline sobre primitivas | `src/builtins/` | widget composto de conveniência |
| **Componente** | Igual ao builtin, mas registrado pelo app | arquivos do app | widget custom do usuário |
| **Diálogo** | Transiente, construído em Rust, sobreposto via `Stack` | `dialogs.rs` | `QDialog`/`QMessageBox` |
| **Motor** | Capacidade de infraestrutura, não um widget | núcleo | `QWidget`/`QLayout`/model-view |

**Regra de decisão:**
- Mapeia direto a um widget do `iced 0.14`? → **Primitiva**.
- Dá para compor de primitivas com só props (sem estado próprio)? → **Builtin**.
- Precisa de estado por instância, canvas custom, ou model/view? → **Componente**
  + provavelmente **bloqueado por um item de Motor** (ver §3).

Base de referência: `iced 0.14` expõe hoje `button, text, text_input,
text_editor, checkbox, toggler, radio, slider, vertical_slider, progress_bar,
pick_list, combo_box, scrollable, container, column, row, space, rule, image,
svg, tooltip, canvas, markdown, qr_code, pane_grid, mouse_area, stack, pin,
hover, themer`. Tudo que **não** está nessa lista tem de ser construído por
composição ou via `canvas` — a coluna **Base iced** sinaliza isso.

### Legenda das tabelas

- **Nível**: `Prim` (primitiva) · `Built` (builtin) · `Comp` (componente) ·
  `Diál` (diálogo) · `Motor` (infra).
- **Estado?**: `—` apresentacional/prop-driven (usável N× hoje) · `◐` estado
  simples controlável por prop (valor + `on_change`) · `●` **exige estado por
  instância** (bloqueado, ver §3).
- **Base iced**: primitiva(s) do `iced` que sustentam o widget, ou `canvas`
  (desenho próprio) / `compõe` (só composição) / `stack` (overlay).
- **Prio**: `P0` fundação/próximo · `P1` alto valor, comum · `P2` importante,
  complexo · `P3` nicho/avançado.
- **Status**: ✅ existe · 🟡 parcial · ⬜ falta.

---

## 2. O catálogo (a "tabela gigante")

### 2.1 Botões e ações

| Qt | Tag glacier-ui | Nível | Base iced | Estado? | Prio | Status | Notas |
|---|---|---|---|---|---|---|---|
| QPushButton | `Button` | Prim | button | — | P0 | ✅ | já existe (`on_click`, estilos GSS) |
| QToolButton | `ToolButton` | Built | button+svg | — | P1 | ⬜ | botão-ícone, variantes flat/menu |
| QRadioButton | `Radio` | Prim | radio | ◐ | P1 | ⬜ | precisa de grupo (valor selecionado compartilhado) |
| QCheckBox | `Checkbox` | Prim | checkbox | ◐ | P0 | ✅ | já existe |
| QCheckBox (tristate) | `Checkbox tristate` | Prim | checkbox | ◐ | P2 | ⬜ | 3º estado indeterminado |
| QCommandLinkButton | `CommandLink` | Built | button+col | — | P2 | ⬜ | título + descrição + seta |
| QDialogButtonBox | `ButtonBox` | Built | row+button | — | P1 | 🟡 | existe nos diálogos; expor como builtin de tela |
| (switch/QML Switch) | `Toggle`/`Toggler` | Prim | toggler | ◐ | P0 | ✅ | já existe |
| QML RoundButton | `RoundButton` | Built | button | — | P3 | ⬜ | border-radius total |
| QML DelayButton | `DelayButton` | Comp | button+canvas | ● | P3 | ⬜ | anel de progresso ao segurar |

### 2.2 Entradas de texto

| Qt | Tag glacier-ui | Nível | Base iced | Estado? | Prio | Status | Notas |
|---|---|---|---|---|---|---|---|
| QLineEdit | `TextInput` | Prim | text_input | ◐ | P0 | ✅ | já existe |
| QLineEdit (password) | `TextInput password` | Prim | text_input | ◐ | P1 | 🟡 | flag `secure` do iced |
| QLineEdit (mask/validator) | `MaskedInput` | Comp | text_input | ● | P2 | ⬜ | máscara + validação (CPF, telefone…) |
| QTextEdit (rich) | `TextEditor` | Prim | text_editor | ● | P1 | ✅ | multi-linha; rich text é limitado |
| QPlainTextEdit | `PlainTextEditor` | Prim | text_editor | ● | P1 | 🟡 | variante sem formatação |
| QTextBrowser | `TextBrowser` | Built | markdown/scrollable | — | P2 | ⬜ | render read-only + links |
| QKeySequenceEdit | `ShortcutInput` | Comp | text_input | ● | P3 | ⬜ | captura combinação de teclas |
| QComboBox (editable) | `ComboEdit` | Prim | combo_box | ● | P1 | 🟡 | `combo_box` do iced permite editar+filtrar |
| — (autocomplete) | `Autocomplete` | Comp | text_input+overlay | ● | P2 | ⬜ | ver QCompleter (§2.12) |

### 2.3 Entradas numéricas e de valor

| Qt | Tag glacier-ui | Nível | Base iced | Estado? | Prio | Status | Notas |
|---|---|---|---|---|---|---|---|
| QSpinBox | `SpinBox` | Comp | text_input+button | ● | P1 | ⬜ | campo + setas ▲▼, min/max/step |
| QDoubleSpinBox | `SpinBox decimals` | Comp | text_input+button | ● | P1 | ⬜ | variante float |
| QSlider | `Slider` | Prim | slider / vertical_slider | ◐ | P1 | ⬜ | min/max/step, orientação |
| QML RangeSlider | `RangeSlider` | Comp | canvas | ● | P2 | ⬜ | dois cursores |
| QDial | `Dial` | Comp | canvas | ● | P2 | ⬜ | knob rotativo |
| QScrollBar | `ScrollBar` | Motor | scrollable | — | P2 | 🟡 | embutido no `scrollable`; expor avulso é raro |
| QProgressBar | `ProgressBar` | Prim | progress_bar | — | P1 | 🟡 | existe em `widget.rs`; formalizar como primitiva |
| QProgressBar (busy) | `Spinner`/`BusyIndicator` | Comp | canvas | ● | P1 | ⬜ | indeterminado (girando) |
| QLCDNumber | `LcdNumber` | Comp | canvas | — | P3 | ⬜ | dígitos estilo display 7-segmentos |
| QML Tumbler | `Tumbler` | Comp | scrollable | ● | P3 | ⬜ | roleta de valores |
| QML Gauge / medidor | `Gauge` | Comp | canvas | ◐ | P2 | ⬜ | medidor circular/arco |

### 2.4 Seleção, listas e árvores (model/view)

| Qt | Tag glacier-ui | Nível | Base iced | Estado? | Prio | Status | Notas |
|---|---|---|---|---|---|---|---|
| QComboBox | `Select` / `Combo` | Prim | pick_list / combo_box | ◐ | P0 | ✅ | ambos existem |
| QFontComboBox | `FontSelect` | Comp | combo_box | ● | P3 | ⬜ | lista fontes do sistema |
| QListWidget | `ListView` | Comp | scrollable+ForEach | ● | P1 | 🟡 | dá para fazer com `for`; falta seleção/estado |
| QListView (model) | `ListView bind` | Motor+Comp | scrollable | ● | P2 | ⬜ | ligado a coleção do contexto |
| QTreeWidget/QTreeView | `TreeView` | Comp | column+recursão | ● | P2 | ⬜ | expandir/recolher = estado por nó |
| QTableWidget/QTableView | `TableView` | Comp | column+row / canvas | ● | P2 | ⬜ | **grande**: cabeçalho, seleção, sort, edição |
| QHeaderView | `TableHeader` | Comp | row+button | ● | P2 | ⬜ | parte da TableView; sort/resize |
| QColumnView | `ColumnView` | Comp | row+ListView | ● | P3 | ⬜ | navegação Miller (finder) |
| QListWidgetItem etc. | (dados, não widget) | — | — | — | — | — | modelados como valores de contexto |
| QCompleter | `Completer` | Comp | overlay+ListView | ● | P2 | ⬜ | popup de sugestões (ver §2.12) |
| QML PageIndicator | `PageIndicator` | Built | row | — | P2 | ⬜ | pontinhos de página |

### 2.5 Data e hora — **foco declarado do projeto**

| Qt | Tag glacier-ui | Nível | Base iced | Estado? | Prio | Status | Notas |
|---|---|---|---|---|---|---|---|
| QCalendarWidget | `Calendar` | Comp | column+row+button | ● | P1 | ⬜ | grade de mês, navegação, dia selecionado |
| QDateEdit / QDatePicker | `DatePicker` | Comp | text_input+Calendar(overlay) | ● | P1 | ⬜ | campo + popup de calendário |
| QTimeEdit | `TimePicker` | Comp | SpinBox×3 / roleta | ● | P1 | ⬜ | hora/min/seg |
| QDateTimeEdit | `DateTimePicker` | Comp | DatePicker+TimePicker | ● | P1 | ⬜ | combinação dos dois |
| — (range) | `DateRangePicker` | Comp | 2×Calendar | ● | P2 | ⬜ | intervalo início→fim |
| — (mês/ano) | `MonthYearPicker` | Comp | pick_list×2 | ● | P3 | ⬜ | seleção só de mês/ano |

> Precisará de uma dependência de datas (`chrono` ou `time`) e provavelmente de
> um tipo de valor de contexto para data (ver "contexto tipado" no ROADMAP).

### 2.6 Displays e indicadores (apresentacionais)

| Qt | Tag glacier-ui | Nível | Base iced | Estado? | Prio | Status | Notas |
|---|---|---|---|---|---|---|---|
| QLabel (texto) | `Text` | Prim | text | — | P0 | ✅ | já existe |
| QLabel (rich/link) | `Link` / `span` | Prim | text/rich | — | P1 | ✅ | `Link` e `span` existem |
| QLabel (imagem) | `Image` | Prim | image | — | P0 | ✅ | já existe |
| — (ícone SVG) | `Svg` / `icone` | Prim | svg | — | P0 | ✅ | já existe |
| — (pílula/rótulo) | `Badge` | Built | container+text | — | P1 | ✅ | builtin canônico |
| — (cartão) | `Card` | Built/Comp | container+col | — | P1 | ✅ | existe (commit 0.35) |
| — (avatar) | `Avatar` | Built | container+image | — | P1 | ⬜ | círculo com imagem/iniciais |
| — (chip removível) | `Chip` | Built | row+button | — | P2 | ⬜ | badge com "×" |
| — (separador) | `Divider` / `Rule` | Prim | rule | — | P0 | ✅ | `Rule` existe |
| QFrame | `Frame` | Built | container | — | P2 | ⬜ | borda/relevo configurável |
| — (skeleton) | `Skeleton` | Built | container | — | P2 | ⬜ | placeholder de carregamento |
| — (QR) | `QrCode` | Prim | qr_code | — | P3 | ⬜ | iced tem nativo |
| QGraphicsView | `Canvas` | Prim | canvas | ● | P3 | ⬜ | superfície de desenho livre |
| QOpenGLWidget | `Shader` | Prim | shader | ● | P3 | ⬜ | iced `shader` (wgpu) |
| — (toast) | `Toast` | Motor | stack | — | P1 | ✅ | já existe (`toasts.rs`) |

### 2.7 Containers e agrupadores

| Qt | Tag glacier-ui | Nível | Base iced | Estado? | Prio | Status | Notas |
|---|---|---|---|---|---|---|---|
| QWidget/QFrame | `Container` | Prim | container | — | P0 | ✅ | já existe |
| QGroupBox | `GroupBox` | Built | container+text | — | P1 | ⬜ | moldura com título (e checkbox opcional) |
| QScrollArea | `Scrollable` / `rolagem` | Prim | scrollable | — | P0 | ✅ | já existe |
| QSplitter | `Splitter` / `PaneGrid` | Prim | pane_grid | ● | P2 | ⬜ | painéis redimensionáveis |
| QToolBox | `ToolBox` | Comp | column+button | ● | P2 | ⬜ | seções empilhadas expansíveis |
| — (accordion) | `Accordion` | Comp | column+button | ● | P1 | ⬜ | itens abre/fecha — **precisa estado** |
| QMdiArea/QMdiSubWindow | `MdiArea` | Comp | canvas/stack | ● | P3 | ⬜ | janelas MDI internas |
| QDockWidget | `Dock` | Comp | pane_grid | ● | P3 | ⬜ | painéis acopláveis |
| — (grupo com scroll) | `Space` | Prim | space | — | P1 | ⬜ | espaçador flexível (iced `space`) |

### 2.8 Navegação (abas, wizard, stacks)

| Qt | Tag glacier-ui | Nível | Base iced | Estado? | Prio | Status | Notas |
|---|---|---|---|---|---|---|---|
| QTabWidget/QTabBar | `Tabs` | Comp | row+button+stack | ● | P1 | ⬜ | aba ativa = estado |
| QStackedWidget | `Stack`/`StackView` | Comp | condicional (`se`) | ◐ | P1 | 🟡 | já dá com `se`; formalizar |
| QWizard/QWizardPage | `Wizard` | Comp | Stack+ButtonBox | ● | P2 | ⬜ | passos com voltar/avançar/finalizar |
| QML SwipeView | `SwipeView` | Comp | stack | ● | P3 | ⬜ | páginas deslizáveis |
| QML Drawer | `Drawer` | Comp | stack+animação | ● | P2 | ⬜ | painel lateral deslizante |
| (roteamento de telas) | `navigate_to` | Motor | — | — | P0 | ✅ | navegação já existe |

### 2.9 Janela principal e barras

| Qt | Tag glacier-ui | Nível | Base iced | Estado? | Prio | Status | Notas |
|---|---|---|---|---|---|---|---|
| QMainWindow | `Window`/`App` | Motor | app | ● | P0 | ✅ | app já é a janela |
| QMenuBar | `MenuBar` | Comp | row+overlay | ● | P2 | ⬜ | menus suspensos |
| QMenu | `Menu` | Comp | overlay+ListView | ● | P2 | ⬜ | popup de itens/ações/submenus |
| — (menu de contexto) | `ContextMenu` | Comp | mouse_area+overlay | ● | P2 | ⬜ | botão direito |
| QToolBar | `ToolBar` | Built | row+ToolButton | — | P2 | ⬜ | faixa de ações |
| QStatusBar | `StatusBar` | Built | row+text | — | P2 | ⬜ | rodapé de status |
| QSystemTrayIcon | `SystemTray` | Motor | (SO) | ● | P3 | ⬜ | depende de suporte do SO/iced |
| QSizeGrip | — | Motor | — | — | P3 | ⬜ | canto de redimensionamento |

### 2.10 Diálogos (módulo `dialogs.rs`)

| Qt | Tag / API glacier-ui | Nível | Base | Estado? | Prio | Status | Notas |
|---|---|---|---|---|---|---|---|
| QMessageBox (info) | `DialogSpec::information` | Diál | stack | — | P0 | ✅ | existe |
| QMessageBox (warning) | `DialogSpec::warning` | Diál | stack | — | P0 | ✅ | existe |
| QMessageBox (critical) | `DialogSpec::error` | Diál | stack | — | P0 | ✅ | existe |
| QMessageBox (question) | `DialogSpec::question` | Diál | stack | — | P0 | ✅ | existe |
| — (confirm) | `DialogSpec::confirm` | Diál | stack | — | P0 | ✅ | existe |
| QInputDialog | `InputDialog` | Diál | stack+TextInput | ● | P1 | ⬜ | pede texto/número/item |
| QProgressDialog | `ProgressDialog` | Diál | stack+ProgressBar | ● | P1 | ⬜ | progresso cancelável |
| QFileDialog (abrir arquivo) | `FileDialog::open` | Diál | **`rfd`** ou nativo | ● | P1 | ⬜ | **foco declarado**; nativo do SO via `rfd` é o caminho pragmático |
| QFileDialog (salvar) | `FileDialog::save` | Diál | `rfd` | ● | P1 | ⬜ | idem |
| QFileDialog (diretório) | `FileDialog::directory` | Diál | `rfd` | ● | P1 | ⬜ | **foco declarado** |
| QColorDialog | `ColorDialog` | Diál | stack+canvas | ● | P2 | ⬜ | roda/HSV/hex |
| QFontDialog | `FontDialog` | Diál | stack+lista | ● | P3 | ⬜ | escolher fonte/tamanho |
| QErrorMessage | (coberto por `error`) | Diál | — | — | — | ✅ | redundante |
| QPrintDialog/QPageSetup | `PrintDialog` | Diál | `rfd`/SO | ● | P3 | ⬜ | impressão (fora do escopo inicial) |

> **Decisão pendente (§4):** diálogos de arquivo/cor nativos (via crate `rfd`)
> vs. construídos em glacier-ui. `rfd` entrega já-funcional e nativo do SO; a
> versão própria dá controle total de estilo mas é cara. Sugestão: `rfd`
> primeiro (P1), versão própria estilizável depois (P3).

### 2.11 Layouts (nível Motor / `parser`)

| Qt | Equivalente glacier-ui | Nível | Estado? | Prio | Status | Notas |
|---|---|---|---|---|---|---|
| QHBoxLayout | `Row` / `row` | Prim | — | P0 | ✅ | existe |
| QVBoxLayout | `Column` / `column` | Prim | — | P0 | ✅ | existe |
| QGridLayout | `Grid` | Prim/Motor | — | P1 | ⬜ | grade linhas×colunas — falta no iced, compor |
| QFormLayout | `Form` / `formulario` | Prim | — | P1 | ✅ | `Form` existe |
| QStackedLayout | `se`/`senao` + Stack | Motor | ◐ | P1 | 🟡 | condicional existe |
| QSpacerItem | `Space` | Prim | — | P1 | ⬜ | espaçador |
| (flow layout) | `Flow`/`Wrap` | Comp | row+quebra | — | P2 | ⬜ | quebra automática de linha |

### 2.12 Overlays, dicas e utilitários

| Qt | Tag glacier-ui | Nível | Base iced | Estado? | Prio | Status | Notas |
|---|---|---|---|---|---|---|---|
| QToolTip | `Tooltip` | Prim | tooltip | — | P1 | ⬜ | iced tem nativo |
| QML ToolTip | (idem) | Prim | tooltip | — | P1 | ⬜ | — |
| QWhatsThis | — | — | — | — | P3 | ⬜ | ajuda contextual (raro) |
| QCompleter | `Completer` | Comp | text_input+overlay | ● | P2 | ⬜ | sugestões enquanto digita |
| QML Popup | `Popup` | Comp | stack | ● | P2 | ⬜ | genérico ancorado |
| — (menu popover) | `Popover` | Comp | stack | ● | P2 | ⬜ | conteúdo flutuante ancorado |
| QSplashScreen | `SplashScreen` | Comp | stack | — | P3 | ⬜ | tela de abertura |
| QRubberBand | — | Comp | canvas | ● | P3 | ⬜ | retângulo de seleção |
| QShortcut/QAction | `Shortcut`/`Action` | Motor | subscription | ● | P2 | ⬜ | atalhos globais de teclado |
| QScroller | (no scrollable) | Motor | scrollable | — | P3 | 🟡 | rolagem por gesto |
| — (badge de notificação) | `NotificationDot` | Built | container | — | P2 | ⬜ | pontinho sobre ícone |

### 2.13 Gráficos e visualização (Qt Charts / DataVisualization)

| Qt | Tag glacier-ui | Nível | Base iced | Estado? | Prio | Status | Notas |
|---|---|---|---|---|---|---|---|
| QChartView (linha) | `LineChart` | Comp | canvas | — | P2 | ⬜ | avaliar crate `plotters`+iced |
| QChartView (barra) | `BarChart` | Comp | canvas | — | P2 | ⬜ | — |
| QChartView (pizza) | `PieChart` | Comp | canvas | — | P2 | ⬜ | — |
| QChartView (área/scatter) | `AreaChart`/`Scatter` | Comp | canvas | — | P3 | ⬜ | — |
| — (sparkline) | `Sparkline` | Comp | canvas | — | P2 | ⬜ | mini-gráfico inline |
| Q3D* (3D bars/scatter) | — | Comp | shader | ● | P3 | ⬜ | escopo distante (wgpu) |

---

## 3. Pré-requisitos do motor (o caminho crítico)

Boa parte da tabela está **bloqueada por infraestrutura**, não por esforço de
markup. A restrição dominante é a coluna **Estado? = ●**:

> **Estado por instância.** Hoje `ctx.set` grava num **único** contexto global —
> duas instâncias do mesmo widget com estado colidiriam (documentado em
> `BUILTINS.md` e no ROADMAP, Fase 2). **Nenhum** widget marcado `●` pode virar
> builtin usável N× sem isso. É o desbloqueio de maior alavancagem: destrava
> `Tabs`, `Accordion`, `SpinBox`, `Calendar`, `DatePicker`, `Slider`, todos os
> pickers de data — o coração do "foco declarado" do projeto.

Ordem sugerida de habilitadores de Motor:

1. **Estado por instância** (`●`) — pré-requisito de ~40% da tabela. **P0.**
2. **`Space` + `Grid`** — layout que falta para telas densas. **P1.**
3. **Sistema de overlay ancorado reutilizável** — `Stack` + posição relativa a
   um widget âncora. Destrava `Menu`, `Popup`, `Popover`, `Completer`,
   `DatePicker` (popup), `Tooltip` custom, `ContextMenu`. Lição de `DIALOGS.md`
   já mapeou os cuidados (`Interaction::Idle` + `on_press` sempre presente). **P1.**
4. **Contexto tipado / valor de data** — reduz `to_string()`/parse manual;
   necessário para pickers de data robustos. **P1.**
5. **Binding a coleção (model/view)** — `ListView`/`TableView`/`TreeView`
   ligados a uma coleção do contexto, com seleção. **P2.**
6. **Canvas exposto como primitiva** — destrava `Dial`, `Gauge`, `Spinner`,
   `ColorDialog`, `LcdNumber`, todos os gráficos. **P2.**
7. **Subscriptions de teclado** — `Shortcut`/`Action` globais. **P2.**

---

## 4. Decisões em aberto

- **Diálogos nativos vs. próprios** (arquivo, cor, fonte): usar o crate `rfd`
  (nativo do SO, pronto) primeiro, ou investir já na versão estilizável própria?
  Recomendação: `rfd` em P1, própria em P3.
- **Dependência de datas**: `chrono` vs. `time` para o módulo de data/hora.
- **Gráficos**: `canvas` na mão vs. integrar `plotters`.
- **Convenção de nomes**: manter aliases PT-BR (`botao`, `seletor`, `rolagem`…)
  para todo widget novo, ou só para o núcleo? (hoje o núcleo tem os dois.)
- **Rich text no `TextBrowser`/`QLabel`**: quanto do HTML/markdown do Qt vale
  reproduzir sobre o `markdown`/`rich` do iced.

---

## 5. Fases de construção (síntese priorizada)

Corte transversal da tabela por prioridade, na ordem que maximiza valor:

**Fase A — fechar o núcleo primitivo (P0/P1 sobre iced direto)**
`Radio` · `Slider` · `ProgressBar` (formalizar) · `Tooltip` · `Space` ·
`Grid` · `password`/`secure` no `TextInput` · `QrCode`. Baixo custo, tudo
mapeia a widget nativo do iced.

**Fase B — destravar estado por instância (Motor P0)**
Sem markup novo; habilita a fase C inteira.

**Fase C — widgets compostos comuns (P1, dependem de estado)**
`Tabs` · `Accordion` · `SpinBox` · `GroupBox` · `ListView` (com seleção) ·
`ToolBar`/`StatusBar` · `Avatar` · `Spinner`/`BusyIndicator`.

**Fase D — data/hora (P1, foco declarado)**
`Calendar` → `DatePicker` → `TimePicker` → `DateTimePicker`. Depende de estado
+ overlay ancorado + valor de data.

**Fase E — diálogos ricos (P1)**
`FileDialog` (open/save/directory via `rfd`) · `InputDialog` · `ProgressDialog`.

**Fase F — overlays e menus (P2)**
Overlay ancorado → `Menu` · `ContextMenu` · `Popover` · `Completer` · `MenuBar`.

**Fase G — model/view pesado (P2)**
`TableView` · `TreeView` · `ColumnView`. O maior investimento; requer binding a
coleção.

**Fase H — canvas e visualização (P2/P3)**
`Dial` · `Gauge` · `ColorDialog` · `LineChart`/`BarChart`/`PieChart` ·
`Sparkline`.

**Fase I — nicho/avançado (P3)**
`MdiArea` · `Dock` · `Wizard` · `SwipeView` · `Drawer` · `SystemTray` ·
`Shader`/3D · impressão.

---

### Resumo numérico

| Categoria | Widgets catalogados | ✅ prontos | 🟡 parciais | ⬜ a fazer |
|---|---|---|---|---|
| Botões e ações | 10 | 3 | 1 | 6 |
| Entradas de texto | 9 | 2 | 3 | 4 |
| Numéricas/valor | 11 | 0 | 2 | 9 |
| Seleção/listas/árvores | 11 | 1 | 1 | 9 |
| Data e hora | 6 | 0 | 0 | 6 |
| Displays/indicadores | 15 | 7 | 0 | 8 |
| Containers | 9 | 3 | 0 | 6 |
| Navegação | 6 | 2 | 1 | 3 |
| Janela/barras | 8 | 1 | 0 | 7 |
| Diálogos | 14 | 6 | 0 | 8 |
| Layouts | 7 | 3 | 1 | 3 |
| Overlays/utilitários | 12 | 0 | 1 | 11 |
| Gráficos | 6 | 0 | 0 | 6 |
| **Total** | **~124** | **28** | **10** | **86** |

O motor já entrega ~23% do catálogo Qt de superfície. O gargalo não é volume de
markup — é o punhado de habilitadores de Motor do §3 (estado por instância +
overlay ancorado + canvas), que sozinhos destravam a maioria dos 86 pendentes.
