use {
    crate::domain::board::{AppBootstrap, BoardCard},
    maud::{DOCTYPE, Markup, PreEscaped, html},
    serde_json::json,
};

const LINCE_LOGO_SVG: &str = include_str!("../../static/lince_logo_white.svg");

pub fn render_app(bootstrap: &AppBootstrap) -> String {
    let bootstrap_json = safe_json_for_html(bootstrap);

    html! {
        (DOCTYPE)
        html lang="pt-BR" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Lince" }
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="";
                link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500&family=IBM+Plex+Sans:wght@400;500;600;700&display=swap";
                link rel="stylesheet" href="/static/styles.css";
                script type="module" src="https://cdn.jsdelivr.net/gh/starfederation/datastar@main/bundles/datastar.js" {}
                script type="module" src="/static/presentation/board/main.js" {}
            }
            body class="startup-active" data-signals=(app_shell_signals(bootstrap)) {
                div id="startup-screen" class="startup-screen" {
                    div class="startup-screen__inner" {
                        div class="startup-hero" {
                            div class="startup-logo" aria-hidden="true" {
                                (PreEscaped(LINCE_LOGO_SVG))
                            }
                            div class="startup-wordmark" { "Lince" }
                            p class="startup-copy" {
                                "Abrindo o board local e preparando os widgets instalados."
                            }
                        }
                        div class="startup-status" id="startup-status-panel" {
                            span class="startup-status__label" { "Loading local workspace" }
                            span id="startup-status-value" class="startup-status__value" { "0%" }
                        }
                        p id="startup-error-message" class="startup-error-message" hidden="" {}
                        div id="startup-progress" class="startup-progress" aria-hidden="true" {
                            div id="startup-progress-fill" class="startup-progress__fill" {}
                        }
                    }
                }
                div class="app-shell" {
                    header class="topbar" {
                        div class="topbar__brand" {
                            div class="brand-mark" aria-hidden="true" {
                                div class="brand-logo" {
                                    (PreEscaped(LINCE_LOGO_SVG))
                                }
                            }
                            div class="brand-lockup" {
                                span class="brand-name" data-text="$appTitle" { (bootstrap.app_name) }
                            }
                        }
                        div class="topbar__actions" {
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
                            div class="pill pill--status" {
                                span class="pill__dot" {}
                                span id="mode-label" { "Dashboard" }
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
                    main class="workspace" {
                        section
                            id="board-shell"
                            class="board-shell"
                            style=(board_style(bootstrap))
                        {
                            div id="board-canvas" class="board-canvas" {
                                div id="board-grid" class="board-grid" aria-hidden="true" {
                                    @for _ in 0..(bootstrap.cols as usize * bootstrap.rows as usize) {
                                        div class="board-grid__cell" {}
                                    }
                                }
                                div id="workspace-empty" class="workspace-empty" hidden="" {
                                    div class="workspace-empty__logo" aria-hidden="true" {
                                        (PreEscaped(LINCE_LOGO_SVG))
                                    }
                                    div class="workspace-empty__eyebrow" { "Espaco livre" }
                                    h2 id="workspace-empty-title" class="workspace-empty__title" { "Sem cards por aqui" }
                                    p id="workspace-empty-copy" class="workspace-empty__copy" {
                                        "Crie uma nova composicao ou entre em modo de edicao para adicionar cards."
                                    }
                                }
                                div id="board-floating-controls" class="board-floating-controls" {
                                    div class="floating-popover" {
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
                                        div id="add-card-popover" class="add-card-popover" hidden="" {
                                            button
                                                id="add-card-import-button"
                                                class="add-card-popover__action"
                                                type="button"
                                            {
                                                span class="add-card-popover__icon" { "↥" }
                                                span class="add-card-popover__copy" {
                                                    strong { "Importar" }
                                                    small { ".lince do disco" }
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
                                        }
                                    }
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
                                div id="drop-zone-overlay" class="drop-zone-overlay" hidden="" {
                                    div class="drop-zone-overlay__panel" {
                                        div id="drop-zone-overlay-eyebrow" class="drop-zone-overlay__eyebrow" { "Import package" }
                                        h2 id="drop-zone-overlay-title" class="drop-zone-overlay__title" { "Solte um arquivo .lince" }
                                        p id="drop-zone-overlay-copy" class="drop-zone-overlay__copy" {
                                            "O package sera lido no backend e aberto em preview antes de virar um card."
                                        }
                                    }
                                }
                                div id="cards-layer" class="cards-layer" {
                                    @for card in &bootstrap.cards {
                                        (render_card(card))
                                    }
                                }
                            }
                        }
                    }
                }
                div id="import-modal-backdrop" class="import-modal-backdrop" hidden="" {
                    section class="import-modal" role="dialog" aria-modal="true" aria-labelledby="import-modal-title" {
                        header class="import-modal__header" {
                            div class="import-modal__lockup" {
                                div class="import-modal__eyebrow" { "External card" }
                                h2 id="import-modal-title" class="import-modal__title" { "Importar package .lince" }
                                p id="import-modal-description" class="import-modal__description" {}
                            }
                            button id="import-close-button" class="import-close-button" type="button" aria-label="Fechar preview do package" { "×" }
                        }
                        div class="import-modal__layout" {
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
                                div class="import-modal__footer" {
                                    button id="import-cancel-button" class="modal-button modal-button--ghost" type="button" { "Cancelar" }
                                    button id="import-confirm-button" class="modal-button modal-button--primary" type="button" { "Adicionar card" }
                                }
                            }
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
                                                sandbox="allow-scripts allow-same-origin"
                                            {}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div id="local-packages-modal-backdrop" class="import-modal-backdrop" hidden="" {
                    section class="import-modal import-modal--catalog" role="dialog" aria-modal="true" aria-labelledby="local-packages-modal-title" {
                        header class="import-modal__header" {
                            div class="import-modal__lockup" {
                                div class="import-modal__eyebrow" { "Local catalog" }
                                h2 id="local-packages-modal-title" class="import-modal__title" { "Widgets instalados" }
                                p class="import-modal__description" {
                                    "Escolha um .lince ja instalado em ~/.config/lince/web/widgets para criar outra copia no workspace atual."
                                }
                            }
                            button id="local-packages-close-button" class="import-close-button" type="button" aria-label="Fechar catalogo local" { "×" }
                        }
                        div class="catalog-toolbar" {
                            div class="catalog-modal__meta" {
                                div class="import-modal__details-label" { "Catalogo local" }
                                p id="local-packages-summary" class="import-modal__details-copy" {
                                    "Carregando widgets instalados..."
                                }
                            }
                            label class="catalog-search" for="local-packages-search" {
                                span class="catalog-search__label" { "Buscar" }
                                input
                                    id="local-packages-search"
                                    class="catalog-search__input"
                                    type="search"
                                    autocomplete="off"
                                    spellcheck="false"
                                    placeholder="Nome, arquivo, autor ou permissao";
                            }
                        }
                        div id="local-package-list" class="local-package-list" {}
                    }
                }
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
                            button id="delete-card-close-button" class="import-close-button" type="button" aria-label="Fechar modal de exclusao" { "×" }
                        }
                        div class="confirm-modal__body" {
                            div class="confirm-modal__card-preview" {
                                span class="confirm-modal__card-label" { "Card" }
                                strong id="delete-card-modal-name" class="confirm-modal__card-name" {}
                            }
                        }
                        div class="confirm-modal__footer" {
                            button id="delete-card-cancel-button" class="modal-button modal-button--ghost" type="button" { "Cancelar" }
                            button id="delete-card-confirm-button" class="modal-button modal-button--danger" type="button" { "Excluir card" }
                        }
                    }
                }
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
                            button id="server-login-close-button" class="import-close-button" type="button" aria-label="Fechar modal de login do servidor" { "×" }
                        }
                        div class="confirm-modal__body" {
                            div class="confirm-modal__card-preview" {
                                span class="confirm-modal__card-label" { "Servidor" }
                                strong id="server-login-server-name" class="confirm-modal__card-name" {}
                            }
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
                        div class="confirm-modal__footer" {
                            button id="server-login-cancel-button" class="modal-button modal-button--ghost" type="button" { "Cancelar" }
                            button id="server-login-confirm-button" class="modal-button modal-button--primary" type="submit" form="server-login-form" { "Conectar" }
                        }
                    }
                }
                div id="widget-config-modal-backdrop" class="import-modal-backdrop" hidden="" {
                    section class="confirm-modal" role="dialog" aria-modal="true" aria-labelledby="widget-config-modal-title" {
                        header class="confirm-modal__header" {
                            div class="confirm-modal__lockup" {
                                div class="confirm-modal__eyebrow" { "Widget settings" }
                                h2 id="widget-config-modal-title" class="confirm-modal__title" { "Configurar widget" }
                                p id="widget-config-modal-description" class="confirm-modal__description" {
                                    "Defina o servidor e os parametros do widget para desbloquear suas integracoes."
                                }
                            }
                            button id="widget-config-close-button" class="import-close-button" type="button" aria-label="Fechar modal de configuracao" { "×" }
                        }
                        div class="confirm-modal__body" {
                            form id="widget-config-form" class="startup-login-form" autocomplete="off" {
                                label class="startup-field" for="widget-config-server-id" {
                                    select id="widget-config-server-id" class="startup-field__input" name="server_id" {
                                        option value="" { "Escolha um servidor" }
                                    }
                                }
                                label id="widget-config-view-id-field" class="startup-field" for="widget-config-view-id" {
                                    input
                                        id="widget-config-view-id"
                                        class="startup-field__input"
                                        type="number"
                                        min="1"
                                        step="1"
                                        name="view_id"
                                        placeholder="View id";
                                }
                                p id="widget-config-help" class="startup-error-message" hidden="" {}
                            }
                        }
                        div class="confirm-modal__footer" {
                            button id="widget-config-cancel-button" class="modal-button modal-button--ghost" type="button" { "Cancelar" }
                            button id="widget-config-save-button" class="modal-button modal-button--primary" type="submit" form="widget-config-form" { "Salvar" }
                        }
                    }
                }
                input id="package-import-input" type="file" accept=".lince,application/zip" hidden="";
                input id="workspace-import-input" type="file" accept=".workspace.lince,application/zip" hidden="";
                script id="lince-bootstrap" type="application/json" { (PreEscaped(bootstrap_json)) }
            }
        }
    }
    .into_string()
}

fn safe_json_for_html<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .expect("bootstrap data should always serialize")
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
}

pub fn render_ai_builder() -> String {
    html! {
        (DOCTYPE)
        html lang="pt-BR" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Lince AI Lab" }
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="";
                link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500&family=IBM+Plex+Sans:wght@400;500;600;700&display=swap";
                link rel="stylesheet" href="/static/styles.css";
                link rel="stylesheet" href="/static/ai-builder.css";
                script type="module" src="/static/presentation/ai/main.js" {}
            }
            body class="ai-builder-body" {
                div class="app-shell app-shell--ai" {
                    header class="topbar" {
                        div class="topbar__brand" {
                            div class="brand-mark" aria-hidden="true" {
                                div class="brand-logo" {
                                    (PreEscaped(LINCE_LOGO_SVG))
                                }
                            }
                            div class="brand-lockup" {
                                span class="brand-name" { "Lince" }
                            }
                        }
                        div class="topbar__actions" {
                            div class="pill pill--ghost" {
                                span class="pill__dot" {}
                                span { "AI widget lab" }
                            }
                            a
                                class="icon-button icon-button--ai"
                                href="/"
                                aria-label="Voltar para o dashboard"
                            {
                                (home_icon())
                            }
                        }
                    }
                    main class="ai-lab" {
                        section class="ai-lab__hero" {
                            div class="ai-lab__hero-copy" {
                                div class="ai-lab__eyebrow" { "Experimental route" }
                                h1 class="ai-lab__title" { "Criar widgets .lince com IA" }
                                p class="ai-lab__description" {
                                    "Descreva um widget, deixe o backend gerar o package completo com OpenAI, revise quantas vezes quiser e exporte o resultado como um .lince pronto para arrastar de volta para o board."
                                }
                            }
                            div class="ai-lab__hero-meta" {
                                div class="ai-metric-card" {
                                    span class="ai-metric-card__label" { "Modelo" }
                                    strong id="ai-model-name" class="ai-metric-card__value" { "gpt-5.4-mini" }
                                    span id="ai-model-summary" class="ai-metric-card__hint" { "Equilibrio entre custo e qualidade para iterar widgets." }
                                }
                                div class="ai-metric-card" {
                                    span class="ai-metric-card__label" { "Custo relativo" }
                                    strong id="ai-model-price" class="ai-metric-card__value" { "1x" }
                                    span id="ai-token-estimate" class="ai-metric-card__hint" { "Geracao: ~8k-16k tokens" }
                                    span id="ai-refine-estimate" class="ai-metric-card__hint ai-metric-card__hint--secondary" { "Refinos: ~4k-9k tokens" }
                                }
                                div class="ai-metric-card" {
                                    span class="ai-metric-card__label" { "Seguranca" }
                                    strong class="ai-metric-card__value" { "API key em memoria" }
                                    span id="ai-key-storage" class="ai-metric-card__hint" { "Some ao reiniciar o backend Rust." }
                                }
                            }
                        }
                        section class="ai-lab__grid" {
                            section class="ai-panel ai-panel--composer" {
                                header class="ai-panel__header" {
                                    div {
                                        div class="ai-panel__eyebrow" { "Prompt" }
                                        h2 class="ai-panel__title" { "Direcao criativa do widget" }
                                    }
                                    button
                                        id="ai-change-key-button"
                                        class="ai-panel__link"
                                        type="button"
                                    {
                                        "API key"
                                    }
                                }
                                div id="ai-status-banner" class="ai-status-banner" {
                                    span id="ai-status-pill" class="ai-status-banner__pill" { "Aguardando API key" }
                                    p id="ai-status-copy" class="ai-status-banner__copy" {
                                        "A chave vive apenas no backend desta sessao. A primeira geracao valida se ela realmente tem acesso ao modelo."
                                    }
                                }
                                section class="ai-models" {
                                    div class="ai-models__header" {
                                        div class="ai-panel__eyebrow" { "Modelos" }
                                        strong class="ai-models__title" { "Escolha o modelo antes de gerar ou revisar" }
                                    }
                                    div id="ai-model-list" class="ai-model-list" {}
                                    p id="ai-model-note" class="ai-form__hint" {
                                        "Os multiplicadores sao estimativas relativas para comparacao rapida entre modelos."
                                    }
                                }
                                div id="ai-error-banner" class="ai-error-banner" hidden="" {}
                                form id="ai-generate-form" class="ai-form" {
                                    label class="ai-form__label" for="ai-prompt-input" { "Pedido" }
                                    textarea
                                        id="ai-prompt-input"
                                        class="ai-form__textarea"
                                        name="prompt"
                                        rows="10"
                                        placeholder="Ex.: crie um widget compacto de notas urgentes com duas colunas, filtros por tag, visual quase monocromatico e um composer minimalista no rodape."
                                    {}
                                    div class="ai-form__hint" {
                                        "O backend envia um preprompt forte com o contrato do .lince, regras visuais do Lince, limites de tamanho e orientacoes de microfrontend."
                                    }
                                    div class="ai-form__actions" {
                                        button id="ai-generate-button" class="modal-button modal-button--primary" type="submit" {
                                            "Gerar widget"
                                        }
                                        button id="ai-reset-draft-button" class="modal-button modal-button--ghost" type="button" {
                                            "Novo draft"
                                        }
                                    }
                                }
                                section class="ai-contract" {
                                    div class="ai-contract__title" { "Contrato enviado para a IA" }
                                    ul class="ai-contract__list" {
                                        li { "O package sempre sai como `index.html` + `config.toml`." }
                                        li { "O `body` do HTML precisa ser o proprio widget, sem moldura extra do host." }
                                        li { "Sem frameworks pesados, sem CDN, sem fetch remoto obrigatorio." }
                                        li { "Estado interno pode usar `localStorage`, sempre namespaced por `data-package-instance-id`." }
                                    }
                                }
                                section class="ai-history" {
                                    div class="ai-history__header" {
                                        div class="ai-panel__eyebrow" { "Session" }
                                        h3 class="ai-history__title" { "Iteracoes" }
                                    }
                                    ul id="ai-history-list" class="ai-history__list" {
                                        li class="ai-history__empty" { "Nenhuma geracao ainda." }
                                    }
                                }
                            }
                            section class="ai-panel ai-panel--preview" {
                                header class="ai-panel__header ai-panel__header--preview" {
                                    div {
                                        div class="ai-panel__eyebrow" { "Preview" }
                                        h2 class="ai-panel__title" { "Resultado do .lince" }
                                    }
                                    a
                                        id="ai-export-link"
                                        class="modal-button modal-button--primary"
                                        href="#"
                                        hidden=""
                                        download=""
                                    {
                                        "Exportar .lince"
                                    }
                                }
                                div class="ai-preview-shell" {
                                    div class="ai-preview-toolbar" {
                                        div class="ai-preview-toolbar__meta" {
                                            strong id="ai-preview-title" class="ai-preview-toolbar__title" { "Sem draft" }
                                            span id="ai-preview-subtitle" class="ai-preview-toolbar__subtitle" { "Gere um widget para ver o preview." }
                                        }
                                        div class="ai-preview-toolbar__aside" {
                                            div id="ai-preview-dimensions" class="ai-preview-toolbar__dimensions" { "3 x 2" }
                                            div class="ai-toggle-group" role="tablist" aria-label="Modo do preview" {
                                                button id="ai-preview-static-button" class="ai-toggle-button is-active" type="button" data-preview-mode="static" aria-pressed="true" { "Estatico" }
                                                button id="ai-preview-edit-button" class="ai-toggle-button" type="button" data-preview-mode="edit" aria-pressed="false" { "Editar" }
                                            }
                                        }
                                    }
                                    div id="ai-preview-edit-controls" class="ai-preview-edit-controls" hidden="" {
                                        div class="ai-size-control" {
                                            span class="ai-size-control__label" { "Largura" }
                                            div class="ai-size-control__actions" {
                                                button id="ai-width-decrease" class="ai-size-control__button" type="button" aria-label="Diminuir largura inicial" { "-" }
                                                strong id="ai-width-value" class="ai-size-control__value" { "3" }
                                                button id="ai-width-increase" class="ai-size-control__button" type="button" aria-label="Aumentar largura inicial" { "+" }
                                            }
                                        }
                                        div class="ai-size-control" {
                                            span class="ai-size-control__label" { "Altura" }
                                            div class="ai-size-control__actions" {
                                                button id="ai-height-decrease" class="ai-size-control__button" type="button" aria-label="Diminuir altura inicial" { "-" }
                                                strong id="ai-height-value" class="ai-size-control__value" { "2" }
                                                button id="ai-height-increase" class="ai-size-control__button" type="button" aria-label="Aumentar altura inicial" { "+" }
                                            }
                                        }
                                        p class="ai-preview-edit-controls__hint" {
                                            "No modo editar o iframe fica travado para voce ajustar o tamanho inicial do widget antes de exportar."
                                        }
                                    }
                                    div id="ai-preview-stage" class="ai-preview-stage" style="--widget-cols:3;--widget-rows:2;" {
                                        div class="ai-preview-grid" aria-hidden="true" {
                                            @for _ in 0..36 {
                                                div class="ai-preview-grid__cell" {}
                                            }
                                        }
                                        div id="ai-preview-empty" class="ai-preview-empty" {
                                            div class="ai-preview-empty__logo" aria-hidden="true" {
                                                (PreEscaped(LINCE_LOGO_SVG))
                                            }
                                            p class="ai-preview-empty__copy" {
                                                "O draft gerado aparece aqui exatamente no formato de um card da grid."
                                            }
                                        }
                                        div id="ai-preview-card" class="ai-preview-card" hidden="" {
                                            iframe
                                                id="ai-preview-frame"
                                                class="ai-preview-frame"
                                                title="Preview do widget gerado por IA"
                                                data-package-instance-id="ai-builder-preview"
                                                sandbox="allow-scripts allow-same-origin"
                                            {}
                                            button class="ai-preview-handle ai-preview-handle--e" type="button" tabindex="-1" aria-hidden="true" data-size-handle="e" {}
                                            button class="ai-preview-handle ai-preview-handle--s" type="button" tabindex="-1" aria-hidden="true" data-size-handle="s" {}
                                            button class="ai-preview-handle ai-preview-handle--se" type="button" tabindex="-1" aria-hidden="true" data-size-handle="se" {}
                                        }
                                    }
                                }
                                div class="ai-preview-meta" {
                                    section class="ai-meta-card" {
                                        div class="ai-meta-card__eyebrow" { "Manifesto" }
                                        dl class="ai-meta-grid" {
                                            div {
                                                dt { "Author" }
                                                dd id="ai-author-value" { "-" }
                                            }
                                            div {
                                                dt { "Version" }
                                                dd id="ai-version-value" { "-" }
                                            }
                                            div {
                                                dt { "Descricao" }
                                                dd id="ai-description-value" { "-" }
                                            }
                                            div {
                                                dt { "Detalhes" }
                                                dd id="ai-details-value" { "-" }
                                            }
                                        }
                                    }
                                    section class="ai-meta-card" {
                                        div class="ai-meta-card__eyebrow" { "Permissoes mock" }
                                        div id="ai-permissions-list" class="ai-permissions-list" {
                                            span class="ai-permissions-list__empty" { "Nenhuma permissao ainda." }
                                        }
                                    }
                                    section class="ai-meta-card" {
                                        div class="ai-meta-card__eyebrow" { "config.toml" }
                                        pre id="ai-config-preview" class="ai-config-preview" { "# O config gerado aparece aqui." }
                                    }
                                    section class="ai-meta-card" {
                                        div class="ai-meta-card__eyebrow" { "Uso da ultima geracao" }
                                        div id="ai-usage-summary" class="ai-usage-summary" { "Sem uso registrado ainda." }
                                    }
                                }
                            }
                        }
                    }
                }
                div id="ai-key-gate" class="ai-key-gate" {
                    section class="ai-key-gate__panel" role="dialog" aria-modal="true" aria-labelledby="ai-key-gate-title" {
                        div class="ai-key-gate__logo" aria-hidden="true" {
                            (PreEscaped(LINCE_LOGO_SVG))
                        }
                        div class="ai-key-gate__eyebrow" { "OpenAI setup" }
                        h2 id="ai-key-gate-title" class="ai-key-gate__title" { "Conecte sua API key para gerar widgets" }
                        p class="ai-key-gate__copy" {
                            "A chave e enviada para o backend Rust e fica apenas em memoria nesta sessao. O host usa essa chave para pedir ao modelo um package `.lince` completo com `index.html` e `config.toml`."
                        }
                        form id="ai-key-form" class="ai-key-form" {
                            label class="ai-form__label" for="ai-api-key" { "OpenAI API key" }
                            input
                                id="ai-api-key"
                                class="ai-key-form__input"
                                type="password"
                                name="api_key"
                                placeholder="sk-..."
                                autocomplete="off";
                            p id="ai-key-form-feedback" class="ai-key-form__hint" {
                                "Geracao inicial costuma ficar em torno de 8k-16k tokens. Refinos geralmente custam menos."
                            }
                            button id="ai-key-submit" class="modal-button modal-button--primary" type="submit" {
                                "Salvar chave e entrar"
                            }
                        }
                    }
                }
            }
        }
    }
    .into_string()
}

fn board_style(bootstrap: &AppBootstrap) -> String {
    format!(
        "--board-cols:{};--board-rows:{};--board-gap:{}px;",
        bootstrap.cols, bootstrap.rows, bootstrap.gap
    )
}

fn app_shell_signals(bootstrap: &AppBootstrap) -> String {
    json!({
        "appTitle": bootstrap.app_name,
    })
    .to_string()
}

fn render_card(card: &BoardCard) -> Markup {
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
            style=(format!("grid-column: {} / span {}; grid-row: {} / span {};", card.x, card.w, card.y, card.h))
        {
            (render_card_delete_button(card.title.as_str()))
            @if is_package {
                (render_package_body(card))
            } @else {
                header class="card-header" {
                    span class="card-eyebrow" { "Widget" }
                    h2 class="card-title" data-card-title="" { (card.title.as_str()) }
                    p class="card-copy" data-card-description="" { (card.description.as_str()) }
                }
                div class="card-body" {
                    (render_text_body(card.text.as_str()))
                }
            }
            (render_resize_handles())
        }
    }
}

fn render_package_body(card: &BoardCard) -> Markup {
    html! {
        div class="package-widget" {
            iframe
                class="package-widget__frame"
                title=(card.title.as_str())
                loading="lazy"
                data-package-instance-id=(card.id.as_str())
                data-lince-server-id=(card.server_id.as_str())
                data-lince-view-id=(card.view_id.map(|value| value.to_string()).unwrap_or_default())
                sandbox="allow-scripts allow-same-origin"
                srcdoc=(card.html.as_str())
            {}
        }
    }
}

fn render_card_delete_button(title: &str) -> Markup {
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

fn render_text_body(text: &str) -> Markup {
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

fn render_resize_handles() -> Markup {
    html! {
        button type="button" class="resize-handle resize-handle--nw" tabindex="-1" aria-hidden="true" data-resize-handle="nw" {}
        button type="button" class="resize-handle resize-handle--ne" tabindex="-1" aria-hidden="true" data-resize-handle="ne" {}
        button type="button" class="resize-handle resize-handle--sw" tabindex="-1" aria-hidden="true" data-resize-handle="sw" {}
        button type="button" class="resize-handle resize-handle--se" tabindex="-1" aria-hidden="true" data-resize-handle="se" {}
        button type="button" class="resize-handle resize-handle--n" tabindex="-1" aria-hidden="true" data-resize-handle="n" {}
        button type="button" class="resize-handle resize-handle--e" tabindex="-1" aria-hidden="true" data-resize-handle="e" {}
        button type="button" class="resize-handle resize-handle--s" tabindex="-1" aria-hidden="true" data-resize-handle="s" {}
        button type="button" class="resize-handle resize-handle--w" tabindex="-1" aria-hidden="true" data-resize-handle="w" {}
    }
}

fn pencil_icon() -> Markup {
    html! {
        svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
            path d="M12 20h9" {}
            path d="M16.5 3.5a2.12 2.12 0 1 1 3 3L7 19l-4 1 1-4Z" {}
        }
    }
}

fn sparkles_icon() -> Markup {
    html! {
        svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
            path d="M12 3l1.45 4.05L17.5 8.5l-4.05 1.45L12 14l-1.45-4.05L6.5 8.5l4.05-1.45L12 3Z" {}
            path d="M5 15l.8 2.2L8 18l-2.2.8L5 21l-.8-2.2L2 18l2.2-.8L5 15Z" {}
            path d="M19 13l.9 2.1L22 16l-2.1.9L19 19l-.9-2.1L16 16l2.1-.9L19 13Z" {}
        }
    }
}

fn home_icon() -> Markup {
    html! {
        svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
            path d="M4 11.5 12 5l8 6.5" {}
            path d="M6 10.5V19h12v-8.5" {}
        }
    }
}

fn eye_icon() -> Markup {
    html! {
        svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
            path d="M2.5 12s3.5-6 9.5-6 9.5 6 9.5 6-3.5 6-9.5 6-9.5-6-9.5-6Z" {}
            circle cx="12" cy="12" r="3.25" {}
        }
    }
}

fn chevron_down_icon() -> Markup {
    html! {
        svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
            path d="m4 6 4 4 4-4" {}
        }
    }
}

fn trash_icon() -> Markup {
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
