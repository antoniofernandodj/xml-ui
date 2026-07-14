//! Tudo de que a avaliação depende **e que o rastreamento de chaves não
//! enxerga** — as folhas de estilo, os templates parseados e o viewport — atrás
//! de um portão único que conta as mudanças.
//!
//! # Por que isto existe
//!
//! O cache de avaliação ([`crate::eval::EvalCache`]) sabe reagir a mudanças no
//! *contexto*: toda leitura é registrada, então uma subárvore só é reusada
//! enquanto as chaves de que ela depende tiverem o mesmo valor. Mas a árvore
//! avaliada **também** depende de três coisas que não são chaves de contexto:
//!
//! - as folhas `.gss` (um `.card { padding: 4 }` que vira `padding: 99`);
//! - o viewport (cruzar um breakpoint de `@media` muda o estilo de tudo);
//! - o markup (um template recarregado a quente).
//!
//! Mudar qualquer uma delas sem descartar o cache faz o motor servir nós com o
//! **estilo velho** — e o pior: sem erro, sem sintoma imediato, sem nada que
//! aponte a causa. Foi a armadilha central do desenho do cache.
//!
//! A primeira versão tratava isso com oito chamadas manuais de
//! `invalidate_eval_cache()`, espalhadas pelos call-sites que mutavam esses
//! campos. Funcionava — e era exatamente o tipo de invariante que sobrevive à
//! revisão e morre seis meses depois, quando alguém acrescenta um nono call-site
//! e não sabe que precisava avisar ninguém. (Uma delas, no hot-reload, já
//! escrevia direto em `stylesheets[idx]` e só não quebrava porque um
//! `invalidate` genérico vinha depois, por sorte.)
//!
//! Aqui os campos são **privados** e moram noutro módulo, então nem o próprio
//! `lib.rs` consegue mutá-los sem passar por um método daqui — e todo método que
//! muta incrementa a [`RenderInputs::epoch`]. O cache guarda a época em que foi
//! construído e se descarta sozinho quando ela avança. A invariante deixa de
//! depender de eu lembrar, e passa a depender do compilador.

use std::collections::HashMap;

use crate::parser::UiNode;
use crate::stylesheet::StyleSheet;

/// Ver o [módulo](self).
pub struct RenderInputs {
    /// Folhas `.gss` globais, em ordem crescente de prioridade.
    stylesheets: Vec<StyleSheet>,
    /// Caminhos das folhas globais, paralelo a `stylesheets` — os dois **têm** de
    /// andar juntos, que é outra razão para não serem `pub`.
    stylesheet_paths: Vec<String>,
    /// Folhas com escopo de componente (`<style scoped>`).
    component_stylesheets: HashMap<String, Vec<StyleSheet>>,
    /// Templates parseados, por nome.
    templates: HashMap<String, UiNode>,
    /// Viewport atual, em px lógicos (avalia as `@media`).
    viewport: (f32, f32),
    /// Contador de mudanças. Qualquer mutação daqui o incrementa; o cache de
    /// avaliação se invalida quando percebe que avançou.
    epoch: u64,
}

impl Default for RenderInputs {
    fn default() -> Self {
        Self {
            stylesheets: Vec::new(),
            stylesheet_paths: Vec::new(),
            component_stylesheets: HashMap::new(),
            templates: HashMap::new(),
            viewport: (1280.0, 800.0),
            epoch: 0,
        }
    }
}

impl RenderInputs {
    /// A época atual. O cache guarda a sua e se descarta quando esta avança.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Marca que algo que a avaliação enxerga mudou. Privado de propósito: só os
    /// métodos de mutação deste módulo o chamam, e é por isso que nenhum
    /// call-site de fora consegue esquecer.
    fn touch(&mut self) {
        self.epoch += 1;
    }

    // ── Leitura ─────────────────────────────────────────────────────────────

    pub fn stylesheets(&self) -> &[StyleSheet] {
        &self.stylesheets
    }

    pub fn component_stylesheets(&self) -> &HashMap<String, Vec<StyleSheet>> {
        &self.component_stylesheets
    }

    pub fn stylesheet_paths(&self) -> &[String] {
        &self.stylesheet_paths
    }

    pub fn templates(&self) -> &HashMap<String, UiNode> {
        &self.templates
    }

    pub fn template(&self, name: &str) -> Option<&UiNode> {
        self.templates.get(name)
    }

    pub fn has_template(&self, name: &str) -> bool {
        self.templates.contains_key(name)
    }

    pub fn viewport(&self) -> (f32, f32) {
        self.viewport
    }

    /// `true` se alguma folha (global ou de escopo) declara seletor de **tag** —
    /// calculado aqui para o motor não pagar por nó no caso comum.
    pub fn has_tag_rules(&self) -> bool {
        self.stylesheets.iter().any(|s| s.has_tag_rules())
            || self
                .component_stylesheets
                .values()
                .flatten()
                .any(|s| s.has_tag_rules())
    }

    /// `true` se mover o viewport de `old` para `new` ativa ou desativa alguma
    /// `@media` — o que decide se vale reavaliar num resize.
    pub fn media_set_changes(&self, old: (f32, f32), new: (f32, f32)) -> bool {
        let sheets = self
            .stylesheets
            .iter()
            .chain(self.component_stylesheets.values().flatten());
        sheets
            .flat_map(|s| &s.media)
            .any(|mq| mq.condition.matches(old.0, old.1) != mq.condition.matches(new.0, new.1))
    }

    // ── Mutação (cada uma avança a época) ───────────────────────────────────

    /// Instala (ou substitui no lugar, pela chave) uma folha global. `key` é o
    /// caminho do arquivo, ou uma chave sintética para um `<style>` inline — é o
    /// que faz o hot-reload trocar a mesma posição em vez de empilhar cópias.
    pub fn install_stylesheet(&mut self, key: String, sheet: StyleSheet) {
        match self.stylesheet_paths.iter().position(|p| *p == key) {
            Some(idx) => self.stylesheets[idx] = sheet,
            None => {
                self.stylesheets.push(sheet);
                self.stylesheet_paths.push(key);
            }
        }
        self.touch();
    }

    /// Define (ou limpa, com uma lista vazia) as folhas com escopo de `component`.
    pub fn set_scoped_stylesheets(&mut self, component: &str, sheets: Vec<StyleSheet>) {
        if sheets.is_empty() {
            self.component_stylesheets.remove(component);
        } else {
            self.component_stylesheets
                .insert(component.to_string(), sheets);
        }
        self.touch();
    }

    pub fn insert_template(&mut self, name: String, ast: UiNode) {
        self.templates.insert(name, ast);
        self.touch();
    }

    /// Só avança a época se o viewport realmente mudou — um resize de um pixel
    /// que não cruza `@media` nenhuma não pode custar um cache inteiro. Devolve
    /// `true` quando a mudança pode ter afetado o estilo (cruzou breakpoint).
    pub fn set_viewport(&mut self, new: (f32, f32)) -> bool {
        if new == self.viewport {
            return false;
        }
        let crossed = self.media_set_changes(self.viewport, new);
        self.viewport = new;
        if crossed {
            self.touch();
        }
        crossed
    }
}
