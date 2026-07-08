/// Uma "pílula" de rótulo — texto curto sobre um fundo arredondado, o clássico
/// selo de status/contagem ("Novo", "3", "Beta"). Puramente apresentacional.
///
/// Props (todas opcionais; o default vem do próprio template via `{prop|def}`):
/// - `badge_text`  — o rótulo. Default: `Badge`.
/// - `badge_bg`    — cor de fundo. Default: `#89B4FA`.
/// - `badge_fg`    — cor do texto. Default: `#11111B`.
/// - `badge_size`  — tamanho do texto (numérico, templado). Default: `13`.
///
/// ```xml
/// <Badge badge_text="Novo" badge_bg="#A6E3A1" badge_size="15" />
/// ```

use crate::component::{Component, Context, Template};

pub struct Badge;

impl Component for Badge {
    fn name(&self) -> &str {
        "Badge"
    }

    fn template(&self) -> Template {
        // Defaults inline via `{prop|default}` — sem semear o contexto global.
        // `size` é numérico e ainda assim aceita `{prop}` (resolvido no eval).
        Template::Inline(
            r#"<Container
                    background="{badge_bg|#89B4FA}"
                    padding="4 10"
                    border_radius="12"
                >
                    <Text
                        content="{badge_text|Badge}"
                        color="{badge_fg|#11111B}"
                        size="{badge_size|13}"
                        bold="true"
                    />
                </Container>"#
                .to_string(),
        )
    }

    fn update(&mut self, _action: &str, _value: Option<&str>, _ctx: &mut Context) {
        // Apresentacional: sem estado, sem comportamento, sem `init`.
    }
}
