mod body;
mod script;
mod styles;

use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};

pub(crate) const FEATURE_FLAG: &str = "sand.kanban_record_view";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "kanban-record-view.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "▥".into(),
            title: "Kanban".into(),
            author: "Lince Labs".into(),
            version: "1.0.0".into(),
            description: "Kanban oficial para acompanhar uma view SSE da tabela record.".into(),
            details: "Resolve o contrato do widget pela instancia do host, consome o stream oficial filtrado do Kanban e persiste ergonomia local do board no card.".into(),
            initial_width: 6,
            initial_height: 5,
            requires_server: true,
            permissions: vec![
                "bridge_state".into(),
                "read_view_stream".into(),
                "write_records".into(),
                "write_table".into(),
            ],
        },
        head_links: vec![],
        inline_styles: styles::INLINE_STYLES.to_vec(),
        body: body::body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script::script())],
    }
}
