mod body;
mod script;

use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};

pub(crate) const FEATURE_FLAG: &str = "sand.markdown_notes";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: r#"note.html"#,
        lang: r#"pt-BR"#,
        manifest: PackageManifest {
            icon: r#"✎"#.into(),
            title: r#"Note"#.into(),
            author: r#"Lince Labs"#.into(),
            version: r#"0.1.0"#.into(),
            description: r#"Nota Markdown baseada em Record."#.into(),
            details: r#"Cria um Record quando recebe titulo, ou edita um Record existente escolhido pelo seletor."#.into(),
            initial_width: 4,
            initial_height: 4,
            requires_server: true,
            permissions: vec!["write_records".into()],
        },
        head_links: vec![],
        inline_styles: crate::sand::record_editor::styles::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
