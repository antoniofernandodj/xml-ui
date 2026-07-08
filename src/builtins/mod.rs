//! Biblioteca de widgets **embutidos** da `glacier-ui`.
//!
//! Ao contrário de um [`Component`] do app — que o host precisa `register`ar —
//! estes componentes a própria lib registra sozinha em [`crate::GlacierUI::new`]
//! (via `register_builtins`), antes de `self` retornar. O efeito é que a tag
//! fica disponível em qualquer template **sem o app configurar nada**: `<Badge/>`
//! funciona igual a uma primitiva `<Button/>`.
//!
//! O passo a passo para adicionar um widget está em `BUILTINS.md`; este
//! módulo documenta as garantias e restrições do motor que moldam o formato de
//! um builtin.
//!
//! # Como um builtin é registrado
//!
//! `GlacierUI::new()` chama `register_builtins()`, que percorre
//! [`builtin_components`] e registra cada um via `register_one` (o caminho
//! interno, sem reavaliar a cada um — a reavaliação acontece quando o app
//! registra as telas). O nome de cada builtin também entra num conjunto
//! (`builtin_component_names`) usado para a regra de override abaixo.
//!
//! Todo builtin usa [`Template::Inline`] — XML **compilado no binário**, nunca
//! lido de disco. Isso torna o parse determinístico: se ele falhar é bug *da
//! lib* (não do app), então `register_builtins` usa `expect`/`panic`, o que
//! mantém `new()` infalível (devolve `Self`, não `Result`). Um builtin que
//! precisasse de I/O ou hot-reload (`Template::File`) violaria essa garantia e
//! exigiria repensar a assinatura de `new()`.
//!
//! # Espaço de nomes e override
//!
//! Builtins vivem no **mesmo espaço de nomes** dos componentes do app (o motor
//! resolve qualquer tag desconhecida por `parsed_templates.get(name)`). Para que
//! um builtin nunca "sequestre" um nome que o app quer usar, uma definição
//! explícita do app **sobrescreve** o builtin de mesmo nome:
//!
//! - `register(Box::new(MeuBadge))` e `register_component("Badge", …)` inserem
//!   por cima no `parsed_templates` e removem o nome de `builtin_component_names`.
//! - `<import name="Badge" from="…"/>`: o guarda de `load_imports` normalmente
//!   pula um nome já carregado, mas abre exceção quando o nome atual é um builtin
//!   — o import assume e o nome deixa de ser builtin.
//!
//! Regra prática: builtin é o padrão; qualquer registro/importe explícito do app
//! ganha. Uma vez sobrescrito, o nome vira um componente comum (imports
//! seguintes do mesmo nome voltam a ser ignorados, como antes).
//!
//! # O que o motor oferece a um widget
//!
//! Dois mecanismos (ver `crate::eval` e [`crate::parser::NumAttr`]) tornam um
//! widget parametrizável sem estado externo:
//!
//! 1. **Props em qualquer atributo.** As props de `<Badge foo="x"/>` são
//!    mescladas num *clone por instância* do contexto durante a avaliação, então
//!    `{foo}` no template resolve por instância. Atributos string (`content`,
//!    `color`, `background`, `width`, `height`, `padding`, …) sempre aceitaram
//!    `{prop}`; os **numéricos** (`size`, `spacing`, `border_radius`,
//!    `border_width`, `max_width`, `max_height`) também — quando o valor contém
//!    `{`, o parser guarda a string crua em `UiNode.numeric_templates` e o eval a
//!    resolve e converte para `f32` na hora certa.
//!
//! 2. **Defaults por instância, sem estado global.** A sintaxe `{prop|default}`
//!    usa `default` quando `prop` não está no contexto. Assim o widget se
//!    auto-configura **sem semear nada no contexto global** em [`Component::init`]
//!    — evitando colisão de chaves entre componentes. Cada instância sobrescreve
//!    só o que quer; o resto cai no default declarado no próprio template.
//!
//! Precedência de um campo, do mais forte ao mais fraco: valor templado (`{…}`
//! resolvido) → literal inline no atributo → valor vindo de uma classe `.gss`.
//!
//! # Restrição em aberto: contexto global único
//!
//! O estado escrito com `ctx.set` num `update`/`init` vai para **um** contexto
//! global, não para um estado por instância. Consequência: um widget **com
//! estado** usado mais de uma vez na mesma tela compartilha esse estado entre as
//! instâncias (elas colidem). Por isso os builtins atuais são **apresentacionais
//! / prop-driven** (Badge e afins): recebem tudo por prop e não guardam estado.
//! Widgets interativos com estado por instância dependem de uma evolução do
//! motor ainda não feita.
//!
//! # Como crescer a biblioteca
//!
//! Escreva o `impl Component` neste arquivo e inclua-o em [`builtin_components`]
//! — é o **único** ponto que o motor lê; nada mais precisa mudar. Guia completo,
//! com checklist e armadilhas, em `BUILTINS.md`.
mod badge;

use crate::component::Component;
use crate::builtins::badge::Badge;

/// Todos os componentes embutidos, na ordem em que o motor os registra.
///
/// **Ponto único de crescimento** da biblioteca: para publicar um widget novo,
/// escreva o `impl Component` neste módulo e adicione um `Box::new(...)` aqui.
/// O motor lê só esta lista (em [`crate::GlacierUI::new`]); nenhuma outra parte
/// da lib precisa saber do widget.
pub fn builtin_components() -> Vec<Box<dyn Component>> {
    vec![Box::new(Badge)]
}

