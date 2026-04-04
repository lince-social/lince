use {
    crate::domain::board::{AppBootstrap, BoardCard},
    maud::{Markup, PreEscaped, html},
    serde::Serialize,
    std::time::{SystemTime, UNIX_EPOCH},
};

const LINCE_LOGO_SVG: &str = include_str!("../../../static/lince_logo_white.svg");

pub(crate) fn safe_json_for_html<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .expect("bootstrap data should always serialize")
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
}

pub(crate) fn asset_version_token() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub(crate) fn board_style(bootstrap: &AppBootstrap) -> String {
    format!(
        "--board-cols:{};--board-rows:{};--board-gap:{}px;",
        bootstrap.cols, bootstrap.rows, bootstrap.gap
    )
}

pub(crate) fn app_shell_signals(bootstrap: &AppBootstrap) -> String {
    serde_json::json!({
        "appTitle": bootstrap.app_name,
    })
    .to_string()
}

pub(crate) fn render_lince_logo() -> Markup {
    html! {
        (PreEscaped(LINCE_LOGO_SVG))
    }
}

pub(crate) fn render_topbar_brand(title: &str, data_text: Option<&str>) -> Markup {
    html! {
        div class="topbar__brand" {
            div class="brand-mark" aria-hidden="true" {
                div class="brand-logo" {
                    (render_lince_logo())
                }
            }
            div class="brand-lockup" {
                @if let Some(data_text) = data_text {
                    span class="brand-name" data-text=(data_text) { (title) }
                } @else {
                    span class="brand-name" { (title) }
                }
            }
        }
    }
}

pub(crate) fn render_card(card: &BoardCard) -> Markup {
    let is_package = card.kind == "package";
    let class_name = if is_package {
        "board-card board-card--package"
    } else {
        "board-card"
    };

    html! {
        article
            class=(class_name)
            data-card-id=(card.id.as_str())
            data-card-kind=(card.kind.as_str())
            style=(format!(
                "grid-column: {} / span {}; grid-row: {} / span {};",
                card.x, card.w, card.y, card.h
            ))
        {
            (render_card_delete_button(card.title.as_str()))
            @if is_package {
                (render_package_body(card))
            } @else {
                (render_text_card_body(card))
            }
            (render_resize_handles())
        }
    }
}

fn render_text_card_body(card: &BoardCard) -> Markup {
    html! {
        header class="card-header" {
            span class="card-eyebrow" { "Widget" }
            h2 class="card-title" data-card-title="" { (card.title.as_str()) }
            p class="card-copy" data-card-description="" { (card.description.as_str()) }
        }
        div class="card-body" {
            (render_text_body(card.text.as_str()))
        }
    }
}

pub(crate) fn render_package_body(card: &BoardCard) -> Markup {
    let frame_src = package_frame_src(card.package_name.as_str());
    html! {
        div class="package-widget" {
            @if card.package_name.trim().is_empty() {
                iframe
                    class="package-widget__frame"
                    title=(card.title.as_str())
                    loading="lazy"
                    data-package-instance-id=(card.id.as_str())
                    data-lince-server-id=(card.server_id.as_str())
                    data-lince-view-id=(card.view_id.map(|value| value.to_string()).unwrap_or_default())
                    sandbox="allow-scripts allow-same-origin allow-pointer-lock allow-popups"
                    allow="fullscreen"
                    allowfullscreen=""
                    srcdoc=(card.html.as_str())
                {}
            } @else {
                iframe
                    class="package-widget__frame"
                    title=(card.title.as_str())
                    loading="lazy"
                    data-package-instance-id=(card.id.as_str())
                    data-lince-server-id=(card.server_id.as_str())
                    data-lince-view-id=(card.view_id.map(|value| value.to_string()).unwrap_or_default())
                    sandbox="allow-scripts allow-same-origin allow-pointer-lock allow-popups"
                    allow="fullscreen"
                    allowfullscreen=""
                    src=(frame_src)
                {}
            }
        }
    }
}

fn package_frame_src(package_name: &str) -> String {
    format!(
        "/host/packages/local/by-filename/{}/content/index.html",
        urlencoding::encode(package_name)
    )
}

pub(crate) fn render_card_delete_button(title: &str) -> Markup {
    html! {
        button
            type="button"
            class="card-delete-button"
            data-card-action="delete"
            aria-label=(format!("Excluir {}", title))
        {
            span class="card-delete-button__icon" { (trash_icon()) }
            span class="card-delete-button__label" { "REMOVER" }
        }
    }
}

pub(crate) fn render_text_body(text: &str) -> Markup {
    html! {
        div class="text-widget" {
            p class="text-widget__content" data-card-text="" { (text) }
            div class="text-widget__meta" {
                span { "snap grid" }
                span { "local layout" }
            }
        }
    }
}

pub(crate) fn render_resize_handles() -> Markup {
    let handles = ["nw", "ne", "sw", "se", "n", "e", "s", "w"];

    html! {
        @for handle in handles {
            button
                type="button"
                class=(format!("resize-handle resize-handle--{handle}"))
                tabindex="-1"
                aria-hidden="true"
                data-resize-handle=(handle)
            {}
        }
    }
}

pub(crate) fn pencil_icon() -> Markup {
    html! {
        svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
            path d="M12 20h9" {}
            path d="M16.5 3.5a2.12 2.12 0 1 1 3 3L7 19l-4 1 1-4Z" {}
        }
    }
}

pub(crate) fn sparkles_icon() -> Markup {
    html! {
        svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
            path d="M12 3l1.45 4.05L17.5 8.5l-4.05 1.45L12 14l-1.45-4.05L6.5 8.5l4.05-1.45L12 3Z" {}
            path d="M5 15l.8 2.2L8 18l-2.2.8L5 21l-.8-2.2L2 18l2.2-.8L5 15Z" {}
            path d="M19 13l.9 2.1L22 16l-2.1.9L19 19l-.9-2.1L16 16l2.1-.9L19 13Z" {}
        }
    }
}

pub(crate) fn home_icon() -> Markup {
    html! {
        svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
            path d="M4 11.5 12 5l8 6.5" {}
            path d="M6 10.5V19h12v-8.5" {}
        }
    }
}

pub(crate) fn eye_icon() -> Markup {
    html! {
        svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
            path d="M2.5 12s3.5-6 9.5-6 9.5 6 9.5 6-3.5 6-9.5 6-9.5-6-9.5-6Z" {}
            circle cx="12" cy="12" r="3.25" {}
        }
    }
}

pub(crate) fn chevron_down_icon() -> Markup {
    html! {
        svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
            path d="m4 6 4 4 4-4" {}
        }
    }
}

pub(crate) fn trash_icon() -> Markup {
    html! {
        svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
            path d="M4 7h16" {}
            path d="M9 3.5h6" {}
            path d="M7 7l1 12h8l1-12" {}
            path d="M10 11v5" {}
            path d="M14 11v5" {}
        }
    }
}
