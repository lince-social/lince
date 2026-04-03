mod body;
mod script;
mod styles;

use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: r#"markdown-notes.html"#,
        lang: r#"pt-BR"#,
        manifest: PackageManifest {
            icon: r#"✎"#.into(),
            title: r#"Markdown Notes"#.into(),
            author: r#"Lince Labs"#.into(),
            version: r#"0.1.0"#.into(),
            description: r#"Bloco de notas em Markdown com alternancia entre texto cru e preview renderizado."#.into(),
            details: r#"Widget minimalista sem moldura: um switch pequeno no topo direito alterna entre edicao raw e renderizacao Markdown."#.into(),
            initial_width: 4,
            initial_height: 4,
            permissions: vec![],
        },
        head_links: vec![],
        inline_styles: styles::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
