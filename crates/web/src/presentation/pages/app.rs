use {
    crate::domain::board::AppBootstrap,
    maud::{DOCTYPE, Markup, PreEscaped, html},
};

use super::shared::{
    app_shell_signals, asset_version_token, board_style, chevron_down_icon, eye_icon, pencil_icon,
    render_card, render_lince_logo, render_topbar_brand, safe_json_for_html, sparkles_icon,
};

pub fn render_app(bootstrap: &AppBootstrap) -> String {
    let bootstrap_json = safe_json_for_html(bootstrap);
    let asset_version = asset_version_token();

    render_app_document(bootstrap, bootstrap_json.as_str(), asset_version).into_string()
}

fn render_app_document(
    bootstrap: &AppBootstrap,
    bootstrap_json: &str,
    asset_version: u64,
) -> Markup {
    html! {
        (DOCTYPE)
        html lang="pt-BR" {
            (render_app_head(asset_version))
            (render_app_body(bootstrap, bootstrap_json))
        }
    }
}

fn render_app_head(asset_version: u64) -> Markup {
    html! {
        head {
            meta charset="utf-8";
            meta name="viewport" content="width=device-width, initial-scale=1";
            title { "Lince" }
            link rel="stylesheet" href=(format!("/static/styles.css?v={asset_version}"));
            script type="module" src=(format!("/static/vendored/datastar.js?v={asset_version}")) {}
            script type="module" src=(format!("/static/presentation/board/main.js?v={asset_version}")) {}
        }
    }
}

fn render_app_body(bootstrap: &AppBootstrap, bootstrap_json: &str) -> Markup {
    html! {
        body class="startup-active" data-signals=(app_shell_signals(bootstrap)) {
            (render_startup_screen())
            (render_app_shell(bootstrap))
            (render_app_modals())
            (render_hidden_inputs(bootstrap_json))
        }
    }
}

fn render_startup_screen() -> Markup {
    html! {
        div id="startup-screen" class="startup-screen" {
            div class="startup-screen__inner" {
                (render_startup_hero())
                (render_startup_status())
                p id="startup-error-message" class="startup-error-message" hidden="" {}
                div id="startup-progress" class="startup-progress" aria-hidden="true" {
                    div id="startup-progress-fill" class="startup-progress__fill" {}
                }
            }
        }
    }
}

fn render_startup_hero() -> Markup {
    html! {
        div class="startup-hero" {
            div class="startup-logo" aria-hidden="true" {
                (render_lince_logo())
            }
            div class="startup-wordmark" { "Lince" }
            p class="startup-copy" {
                "Abrindo o board local e preparando os widgets instalados."
            }
        }
    }
}

fn render_startup_status() -> Markup {
    html! {
        div class="startup-status" id="startup-status-panel" {
            span class="startup-status__label" { "Loading local workspace" }
            span id="startup-status-value" class="startup-status__value" { "0%" }
        }
    }
}

fn render_app_shell(bootstrap: &AppBootstrap) -> Markup {
    html! {
        div class="app-shell" {
            (render_topbar(bootstrap))
            (render_workspace_main(bootstrap))
        }
    }
}

fn render_topbar(bootstrap: &AppBootstrap) -> Markup {
    html! {
        header class="topbar" {
            (render_topbar_brand(bootstrap.app_name, Some("$appTitle")))
            (render_topbar_actions())
        }
    }
}

fn render_topbar_actions() -> Markup {
    html! {
        div class="topbar__actions" {
            (render_workspace_switcher())
            div class="pill pill--status" {
                span class="pill__dot" {}
                span id="mode-label" { "Dashboard" }
            }
            button
                id="streams-toggle"
                class="pill pill--ghost topbar-streams-toggle"
                type="button"
                aria-label="Pausar todos os streams"
                aria-pressed="true"
            {
                span class="pill__dot" {}
                span id="streams-toggle-label" { "Streams on" }
            }
            button
                id="edit-toggle"
                class="icon-button"
                type="button"
                aria-label="Alternar modo de edicao"
                aria-pressed="false"
            {
                (pencil_icon())
            }
            a
                class="icon-button icon-button--ai"
                href="/ai"
                aria-label="Abrir criador experimental de widgets com IA"
            {
                (sparkles_icon())
            }
        }
    }
}

fn render_workspace_switcher() -> Markup {
    html! {
        div class="workspace-switcher" {
            button
                id="workspace-toggle"
                class="workspace-indicator"
                type="button"
                aria-label="Abrir seletor de areas"
                aria-expanded="false"
                aria-controls="workspace-popover"
            {
                span id="workspace-current" class="workspace-indicator__value" { "01" }
                span class="workspace-indicator__chevron" aria-hidden="true" {
                    (chevron_down_icon())
                }
            }
            div id="workspace-popover" class="workspace-popover" hidden="" {
                div id="workspace-list" class="workspace-list" {}
                div class="workspace-popover__footer" {
                    button
                        id="add-workspace-button"
                        class="workspace-popover__action"
                        type="button"
                        aria-label="Criar nova area de trabalho"
                    {
                        span class="workspace-popover__action-plus" { "+" }
                        span { "Nova area" }
                    }
                    button
                        id="import-workspace-button"
                        class="workspace-popover__action workspace-popover__action--subtle"
                        type="button"
                    {
                        span { "Importar area" }
                    }
                    button
                        id="export-workspace-button"
                        class="workspace-popover__action workspace-popover__action--subtle"
                        type="button"
                    {
                        span { "Exportar area" }
                    }
                }
            }
        }
    }
}

fn render_workspace_main(bootstrap: &AppBootstrap) -> Markup {
    html! {
        main class="workspace" {
            (render_board_shell(bootstrap))
        }
    }
}

fn render_board_shell(bootstrap: &AppBootstrap) -> Markup {
    html! {
        section
            id="board-shell"
            class="board-shell"
            style=(board_style(bootstrap))
        {
            (render_board_canvas(bootstrap))
        }
    }
}

fn render_board_canvas(bootstrap: &AppBootstrap) -> Markup {
    html! {
        div id="board-canvas" class="board-canvas" {
            (render_board_grid(bootstrap))
            (render_workspace_empty())
            (render_board_floating_controls())
            (render_drop_zone_overlay())
            (render_cards_layer(bootstrap))
        }
    }
}

fn render_board_grid(bootstrap: &AppBootstrap) -> Markup {
    let cell_count = bootstrap.cols as usize * bootstrap.rows as usize;

    html! {
        div id="board-grid" class="board-grid" aria-hidden="true" {
            @for _ in 0..cell_count {
                div class="board-grid__cell" {}
            }
        }
    }
}

fn render_workspace_empty() -> Markup {
    html! {
        div id="workspace-empty" class="workspace-empty" hidden="" {
            div class="workspace-empty__logo" aria-hidden="true" {
                (render_lince_logo())
            }
            div class="workspace-empty__eyebrow" { "Espaco livre" }
            h2 id="workspace-empty-title" class="workspace-empty__title" { "Sem cards por aqui" }
            p id="workspace-empty-copy" class="workspace-empty__copy" {
                "Crie uma nova composicao ou entre em modo de edicao para adicionar cards."
            }
        }
    }
}

fn render_board_floating_controls() -> Markup {
    html! {
        div id="board-floating-controls" class="board-floating-controls" {
            div class="floating-popover" {
                (render_add_card_button())
                (render_add_card_popover())
            }
            (render_density_tag())
        }
    }
}

fn render_add_card_button() -> Markup {
    html! {
        button
            id="add-card-button"
            class="floating-tag floating-tag--action"
            type="button"
            hidden
            aria-expanded="false"
            aria-controls="add-card-popover"
        {
            span class="floating-tag__plus" { "+" }
            "Add card"
        }
    }
}

fn render_add_card_popover() -> Markup {
    html! {
        div id="add-card-popover" class="add-card-popover" hidden="" {
            button
                id="add-card-import-button"
                class="add-card-popover__action"
                type="button"
            {
                span class="add-card-popover__icon" { "↥" }
                span class="add-card-popover__copy" {
                    strong { "Importar" }
                    small { "widget .html, .sand ou .lince do disco" }
                }
            }
            button
                id="add-card-local-button"
                class="add-card-popover__action"
                type="button"
            {
                span class="add-card-popover__icon" { "◎" }
                span class="add-card-popover__copy" {
                    strong { "Local" }
                    small { "Catalogo instalado" }
                }
            }
            button
                id="add-card-dna-button"
                class="add-card-popover__action"
                type="button"
            {
                span class="add-card-popover__icon" { "◌" }
                span class="add-card-popover__copy" {
                    strong { "DNA" }
                    small { "Sand publicados nos organs conectados" }
                }
            }
        }
    }
}

fn render_density_tag() -> Markup {
    html! {
        label
            id="density-tag"
            class="floating-tag floating-tag--slider"
            for="density-slider"
            hidden=""
        {
            span class="floating-tag__label" { "Grid" }
            input
                id="density-slider"
                class="density-slider"
                type="range"
                min="1"
                max="7"
                step="1"
                value="4";
            span id="density-value" class="density-control__value" { "16 x 10" }
        }
    }
}

fn render_drop_zone_overlay() -> Markup {
    html! {
        div id="drop-zone-overlay" class="drop-zone-overlay" hidden="" {
            div class="drop-zone-overlay__panel" {
                div id="drop-zone-overlay-eyebrow" class="drop-zone-overlay__eyebrow" { "Import widget" }
                h2 id="drop-zone-overlay-title" class="drop-zone-overlay__title" { "Solte um widget .html, .sand ou .lince" }
                p id="drop-zone-overlay-copy" class="drop-zone-overlay__copy" {
                    "O widget sera lido no backend e aberto em preview antes de virar um card."
                }
            }
        }
    }
}

fn render_cards_layer(bootstrap: &AppBootstrap) -> Markup {
    html! {
        div id="cards-layer" class="cards-layer" {
            @for card in &bootstrap.cards {
                (render_card(card))
            }
        }
    }
}

fn render_app_modals() -> Markup {
    html! {
        (render_import_modal_backdrop())
        (render_local_packages_modal_backdrop())
        (render_dna_packages_modal_backdrop())
        (render_delete_card_modal_backdrop())
        (render_server_login_modal_backdrop())
        (render_widget_config_modal_backdrop())
    }
}

fn render_import_modal_backdrop() -> Markup {
    html! {
        div id="import-modal-backdrop" class="import-modal-backdrop" hidden="" {
            section class="import-modal" role="dialog" aria-modal="true" aria-labelledby="import-modal-title" {
                header class="import-modal__header" {
                    div class="import-modal__lockup" {
                        div class="import-modal__eyebrow" { "External card" }
                        h2 id="import-modal-title" class="import-modal__title" { "Importar widget" }
                        p id="import-modal-description" class="import-modal__description" {}
                    }
                    (render_modal_close_button("import-close-button", "Fechar preview do widget"))
                }
                div class="import-modal__layout" {
                    (render_import_modal_sidebar())
                    (render_import_modal_preview_pane())
                }
            }
        }
    }
}

fn render_import_modal_sidebar() -> Markup {
    html! {
        aside class="import-modal__sidebar" {
            div class="import-modal__meta" {
                div class="import-modal__meta-item" {
                    span class="import-modal__meta-label" { "Arquivo" }
                    strong id="import-package-name" class="import-modal__meta-value" {}
                }
                div class="import-modal__meta-item" {
                    span class="import-modal__meta-label" { "Author" }
                    strong id="import-author" class="import-modal__meta-value" {}
                }
                div class="import-modal__meta-item" {
                    span class="import-modal__meta-label" { "Version" }
                    strong id="import-version" class="import-modal__meta-value" {}
                }
                div class="import-modal__meta-item" {
                    span class="import-modal__meta-label" { "Initial size" }
                    strong id="import-size" class="import-modal__meta-value" {}
                }
            }
            div class="import-modal__details" {
                div class="import-modal__details-label" { "Sobre este widget" }
                p id="import-modal-details" class="import-modal__details-copy" {}
            }
            div class="import-modal__permissions" {
                div class="import-modal__permissions-label" { "Permissoes solicitadas" }
                ul id="import-permissions-list" class="permission-list" {}
            }
            (render_modal_footer(
                "import-cancel-button",
                "Cancelar",
                "import-confirm-button",
                "Adicionar card",
                "modal-button--primary",
                "button",
                None,
            ))
        }
    }
}

fn render_import_modal_preview_pane() -> Markup {
    html! {
        section class="import-modal__preview-pane" {
            div class="import-modal__preview-header" {
                div class="import-modal__preview-title" { "Grid preview" }
                div id="import-preview-density" class="import-modal__preview-density" {}
            }
            div class="import-modal__preview-stage" {
                div id="import-preview-cells" class="import-preview-cells" aria-hidden="true" {}
                div id="import-preview-overlay" class="import-preview-overlay" {
                    div id="import-preview-card" class="import-preview-card" {
                        iframe
                            id="import-preview-frame"
                            class="import-preview-frame"
                            title="Preview do card importado"
                            data-package-instance-id="preview"
                            sandbox="allow-scripts allow-same-origin allow-pointer-lock allow-popups"
                            allow="fullscreen; webgpu"
                            allowfullscreen=""
                        {}
                    }
                }
            }
        }
    }
}

fn render_local_packages_modal_backdrop() -> Markup {
    html! {
        div id="local-packages-modal-backdrop" class="import-modal-backdrop" hidden="" {
            section class="import-modal import-modal--catalog" role="dialog" aria-modal="true" aria-labelledby="local-packages-modal-title" {
                header class="import-modal__header" {
                    div class="import-modal__lockup" {
                        div class="import-modal__eyebrow" { "Local catalog" }
                        h2 id="local-packages-modal-title" class="import-modal__title" { "Catalogo de widgets" }
                        p class="import-modal__description" {
                            "Escolha um widget oficial ou um widget salvo em ~/.config/lince/web/sand para criar outra copia no workspace atual."
                        }
                    }
                    (render_modal_close_button("local-packages-close-button", "Fechar catalogo local"))
                }
                (render_catalog_toolbar(
                    "local-packages-summary",
                    "Catalogo",
                    "Carregando o catalogo de widgets...",
                    "local-packages-search",
                    "Nome, arquivo, autor ou permissao",
                ))
                div id="local-package-list" class="local-package-list" {}
            }
        }
    }
}

fn render_dna_packages_modal_backdrop() -> Markup {
    html! {
        div id="dna-packages-modal-backdrop" class="import-modal-backdrop" hidden="" {
            section class="import-modal import-modal--catalog" role="dialog" aria-modal="true" aria-labelledby="dna-packages-modal-title" {
                header class="import-modal__header" {
                    div class="import-modal__lockup" {
                        div class="import-modal__eyebrow" { "DNA catalog" }
                        h2 id="dna-packages-modal-title" class="import-modal__title" { "Catalogo distribuido de sand" }
                        p class="import-modal__description" {
                            "Busque sand publicados nos organs acessiveis, veja a origem de cada um e instale uma copia local no seu catalogo."
                        }
                    }
                    (render_modal_close_button(
                        "dna-packages-close-button",
                        "Fechar catalogo distribuido de sand",
                    ))
                }
                (render_catalog_toolbar(
                    "dna-packages-summary",
                    "DNA",
                    "Carregando o catalogo distribuido de sand...",
                    "dna-packages-search",
                    "Nome, body, slug, origem ou categoria",
                ))
                div class="catalog-toolbar catalog-toolbar--secondary" {
                    label class="catalog-filter" for="dna-origin-filter" {
                        span class="import-modal__details-label" { "Origem" }
                        select id="dna-origin-filter" class="modal-input" {
                            option value="" { "Todas as origens" }
                        }
                    }
                }
                div id="dna-package-list" class="local-package-list" {}
            }
        }
    }
}

fn render_catalog_toolbar(
    summary_id: &str,
    summary_label: &str,
    summary_text: &str,
    search_id: &str,
    search_placeholder: &str,
) -> Markup {
    html! {
        div class="catalog-toolbar" {
            div class="catalog-modal__meta" {
                div class="import-modal__details-label" { (summary_label) }
                p id=(summary_id) class="import-modal__details-copy" {
                    (summary_text)
                }
            }
            label class="catalog-search" for=(search_id) {
                span class="catalog-search__label" { "Buscar" }
                input
                    id=(search_id)
                    class="catalog-search__input"
                    type="search"
                    autocomplete="off"
                    spellcheck="false"
                    placeholder=(search_placeholder);
            }
        }
    }
}

fn render_delete_card_modal_backdrop() -> Markup {
    html! {
        div id="delete-card-modal-backdrop" class="confirm-modal-backdrop" hidden="" {
            section class="confirm-modal" role="dialog" aria-modal="true" aria-labelledby="delete-card-modal-title" {
                header class="confirm-modal__header" {
                    div class="confirm-modal__lockup" {
                        div class="confirm-modal__eyebrow" { "Delete card" }
                        h2 id="delete-card-modal-title" class="confirm-modal__title" { "Excluir card?" }
                        p id="delete-card-modal-description" class="confirm-modal__description" {
                            "Esse card sera removido do workspace atual."
                        }
                    }
                    (render_modal_close_button("delete-card-close-button", "Fechar modal de exclusao"))
                }
                div class="confirm-modal__body" {
                    div class="confirm-modal__card-preview" {
                        span class="confirm-modal__card-label" { "Card" }
                        strong id="delete-card-modal-name" class="confirm-modal__card-name" {}
                    }
                }
                (render_modal_footer(
                    "delete-card-cancel-button",
                    "Cancelar",
                    "delete-card-confirm-button",
                    "Excluir card",
                    "modal-button--danger",
                    "button",
                    None,
                ))
            }
        }
    }
}

fn render_server_login_modal_backdrop() -> Markup {
    html! {
        div id="server-login-modal-backdrop" class="confirm-modal-backdrop" hidden="" {
            section class="confirm-modal" role="dialog" aria-modal="true" aria-labelledby="server-login-modal-title" {
                header class="confirm-modal__header" {
                    div class="confirm-modal__lockup" {
                        div class="confirm-modal__eyebrow" { "Server login" }
                        h2 id="server-login-modal-title" class="confirm-modal__title" { "Conectar servidor" }
                        p id="server-login-modal-description" class="confirm-modal__description" {
                            "Use suas credenciais desse servidor para desbloquear os widgets dependentes."
                        }
                    }
                    (render_modal_close_button(
                        "server-login-close-button",
                        "Fechar modal de login do servidor",
                    ))
                }
                div class="confirm-modal__body" {
                    div class="confirm-modal__card-preview" {
                        span class="confirm-modal__card-label" { "Servidor" }
                        strong id="server-login-server-name" class="confirm-modal__card-name" {}
                    }
                    (render_server_login_form())
                }
                (render_modal_footer(
                    "server-login-cancel-button",
                    "Cancelar",
                    "server-login-confirm-button",
                    "Conectar",
                    "modal-button--primary",
                    "submit",
                    Some("server-login-form"),
                ))
            }
        }
    }
}

fn render_server_login_form() -> Markup {
    html! {
        form id="server-login-form" class="startup-login-form" autocomplete="on" {
            label class="startup-field" for="server-login-username" {
                input
                    id="server-login-username"
                    class="startup-field__input"
                    type="text"
                    name="username"
                    autocomplete="username"
                    placeholder="Login";
            }
            label class="startup-field" for="server-login-password" {
                div class="startup-password-field" {
                    input
                        id="server-login-password"
                        class="startup-field__input startup-field__input--password"
                        type="password"
                        name="password"
                        autocomplete="current-password"
                        placeholder="Senha";
                    button
                        id="server-login-password-toggle"
                        class="startup-password-toggle"
                        type="button"
                        aria-label="Mostrar senha"
                        aria-pressed="false"
                    {
                        (eye_icon())
                    }
                }
            }
            p id="server-login-error-message" class="startup-error-message" hidden="" {}
        }
    }
}

fn render_widget_config_modal_backdrop() -> Markup {
    html! {
        div id="widget-config-modal-backdrop" class="import-modal-backdrop" hidden="" {
            section class="confirm-modal confirm-modal--fullscreen widget-config-modal" role="dialog" aria-modal="true" aria-labelledby="widget-config-modal-title" {
                header class="confirm-modal__header" {
                    div class="confirm-modal__lockup" {
                        div class="confirm-modal__eyebrow" { "Widget settings" }
                        h2 id="widget-config-modal-title" class="confirm-modal__title" { "Configurar widget" }
                        p id="widget-config-modal-description" class="confirm-modal__description" {
                            "Defina o servidor e os parametros do widget para desbloquear suas integracoes."
                        }
                    }
                    (render_modal_close_button(
                        "widget-config-close-button",
                        "Fechar modal de configuracao",
                    ))
                }
                div class="confirm-modal__body confirm-modal__body--scroll widget-config-modal__body" {
                    (render_widget_config_form())
                }
                (render_modal_footer(
                    "widget-config-cancel-button",
                    "Cancelar",
                    "widget-config-save-button",
                    "Salvar",
                    "modal-button--primary",
                    "submit",
                    Some("widget-config-form"),
                ))
            }
        }
    }
}

fn render_widget_config_form() -> Markup {
    html! {
        form id="widget-config-form" class="widget-config-form" autocomplete="off" {
            div class="widget-config-grid" {
                section class="widget-config-section" {
                    div class="widget-config-section__head" {
                        div class="import-modal__details-label" { "Host" }
                        p class="import-modal__details-copy" { "Escolha o servidor, conecte se preciso e selecione uma view." }
                    }
                    label id="widget-config-server-id-field" class="startup-field" for="widget-config-server-id" {
                        select id="widget-config-server-id" class="startup-field__input" name="server_id" {
                            option value="" { "Escolha um servidor" }
                        }
                    }
                    div id="widget-config-auth-field" class="startup-field startup-field--stack" hidden="" {
                        label class="startup-field startup-field--checkbox" for="widget-config-auth-enabled" {
                            input
                                id="widget-config-auth-enabled"
                                class="startup-field__checkbox"
                                type="checkbox"
                                name="auth_required"
                                disabled="";
                            span class="startup-field__checkbox-copy" {
                                strong { "Server requires login" }
                                small { "Enter credentials here only if the selected server is not connected yet." }
                            }
                        }
                        label class="startup-field" for="widget-config-auth-username" {
                            input
                                id="widget-config-auth-username"
                                class="startup-field__input"
                                type="text"
                                autocomplete="username"
                                placeholder="Login";
                        }
                        label class="startup-field" for="widget-config-auth-password" {
                            div class="startup-password-field" {
                                input
                                    id="widget-config-auth-password"
                                    class="startup-field__input startup-field__input--password"
                                    type="password"
                                    autocomplete="current-password"
                                    placeholder="Senha";
                                button
                                    id="widget-config-auth-password-toggle"
                                    class="startup-password-toggle"
                                    type="button"
                                    aria-label="Mostrar senha"
                                    aria-pressed="false"
                                {
                                    (eye_icon())
                                }
                            }
                        }
                        button
                            id="widget-config-auth-login"
                            class="modal-button modal-button--primary"
                            type="button"
                        {
                            "Conectar"
                        }
                        p id="widget-config-auth-help" class="startup-error-message" hidden="" {}
                    }
                }
                section class="widget-config-section" {
                    div class="widget-config-section__head" {
                        div class="import-modal__details-label" { "Views" }
                        p class="import-modal__details-copy" { "Busque por nome ou id. A lista filtra no cliente enquanto voce digita." }
                    }
                    div id="widget-config-view-field" class="startup-field startup-field--stack" hidden="" {
                        label class="startup-field" for="widget-config-view-search" {
                            input
                                id="widget-config-view-search"
                                class="startup-field__input"
                                type="search"
                                autocomplete="off"
                                spellcheck="false"
                                placeholder="Buscar views por nome ou id";
                        }
                        div class="startup-field__meta" id="widget-config-view-summary" {}
                        div id="widget-config-view-list" class="widget-config-view-list" aria-live="polite" {}
                        p id="widget-config-view-help" class="startup-error-message" hidden="" {}
                    }
                    label id="widget-config-view-id-field" class="startup-field" for="widget-config-view-id" hidden="" {
                        input
                            id="widget-config-view-id"
                            class="startup-field__input"
                            type="number"
                            min="1"
                            step="1"
                            name="view_id"
                            placeholder="View id";
                    }
                }
                section class="widget-config-section" {
                    div class="widget-config-section__head" {
                        div class="import-modal__details-label" { "Behavior" }
                        p class="import-modal__details-copy" { "Mantenha o stream e o preview compilado sob seu controle." }
                    }
                    label
                        id="widget-config-streams-field"
                        class="startup-field startup-field--checkbox"
                        for="widget-config-streams-enabled"
                    {
                        input
                            id="widget-config-streams-enabled"
                            class="startup-field__checkbox"
                            type="checkbox"
                            name="streams_enabled"
                            checked="";
                        span class="startup-field__checkbox-copy" {
                            strong { "Manter stream ativo" }
                            small { "Desative para pausar apenas esse widget sem perder sua configuracao." }
                        }
                    }
                    div id="widget-config-preview-field" class="startup-field startup-field--stack" hidden="" {
                        label class="startup-field startup-field--checkbox" for="widget-config-watch-enabled" {
                            input
                                id="widget-config-watch-enabled"
                                class="startup-field__checkbox"
                                type="checkbox"
                                name="watch_compiled_html";
                            span class="startup-field__checkbox-copy" {
                                strong { "Auto swap compiled HTML" }
                                small { "When edit mode is active, reload this package iframe when the Rust-compiled HTML changes." }
                            }
                        }
                        button
                            id="widget-config-refresh-button"
                            class="modal-button modal-button--ghost"
                            type="button"
                        {
                            "Swap once"
                        }
                        p id="widget-config-preview-help" class="startup-error-message" hidden="" {}
                    }
                    p id="widget-config-help" class="startup-error-message" hidden="" {}
                }
            }
        }
    }
}

fn render_modal_close_button(button_id: &str, aria_label: &str) -> Markup {
    html! {
        button
            id=(button_id)
            class="import-close-button"
            type="button"
            aria-label=(aria_label)
        {
            "×"
        }
    }
}

fn render_modal_footer(
    cancel_button_id: &str,
    cancel_label: &str,
    confirm_button_id: &str,
    confirm_label: &str,
    confirm_variant: &str,
    confirm_type: &str,
    confirm_form: Option<&str>,
) -> Markup {
    html! {
        div class="confirm-modal__footer" {
            button id=(cancel_button_id) class="modal-button modal-button--ghost" type="button" { (cancel_label) }
            @if let Some(form) = confirm_form {
                button
                    id=(confirm_button_id)
                    class=(format!("modal-button {confirm_variant}"))
                    type=(confirm_type)
                    form=(form)
                {
                    (confirm_label)
                }
            } @else {
                button
                    id=(confirm_button_id)
                    class=(format!("modal-button {confirm_variant}"))
                    type=(confirm_type)
                {
                    (confirm_label)
                }
            }
        }
    }
}

fn render_hidden_inputs(bootstrap_json: &str) -> Markup {
    html! {
        input id="package-import-input" type="file" accept=".html,.sand,.lince,text/html,application/zip" hidden="";
        input id="workspace-import-input" type="file" accept=".workspace.sand,.workspace.lince,application/zip" hidden="";
        script id="lince-bootstrap" type="application/json" { (PreEscaped(bootstrap_json)) }
    }
}
