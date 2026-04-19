use {
    super::shared::{asset_version_token, home_icon, render_lince_logo, render_topbar_brand},
    crate::infrastructure::prompt_fragments::load_widget_builder_contract_summaries,
    maud::{DOCTYPE, Markup, html},
};

pub fn render_ai_builder() -> String {
    let asset_version = asset_version_token();
    let contract_items = load_widget_builder_contract_summaries()
        .unwrap_or_else(|error| vec![format!("Falha ao carregar prompt fragments: {error}")]);

    render_ai_document(asset_version, &contract_items).into_string()
}

fn render_ai_document(asset_version: u64, contract_items: &[String]) -> Markup {
    html! {
        (DOCTYPE)
        html lang="pt-BR" {
            (render_ai_head(asset_version))
            (render_ai_body(contract_items))
        }
    }
}

fn render_ai_head(asset_version: u64) -> Markup {
    html! {
        head {
            meta charset="utf-8";
            meta name="viewport" content="width=device-width, initial-scale=1";
            title { "Lince AI Lab" }
            link rel="icon" href="/favicon.ico";
            link rel="shortcut icon" href="/favicon.ico";
            link rel="preconnect" href="https://fonts.googleapis.com";
            link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="";
            link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500&family=IBM+Plex+Sans:wght@400;500;600;700&display=swap";
            link rel="stylesheet" href=(format!("/static/styles.css?v={asset_version}"));
            link rel="stylesheet" href=(format!("/static/ai-builder.css?v={asset_version}"));
            script type="module" src=(format!("/static/presentation/ai/main.js?v={asset_version}")) {}
        }
    }
}

fn render_ai_body(contract_items: &[String]) -> Markup {
    html! {
        body class="ai-builder-body" {
            (render_ai_shell(contract_items))
            (render_ai_key_gate())
        }
    }
}

fn render_ai_shell(contract_items: &[String]) -> Markup {
    html! {
        div class="app-shell app-shell--ai" {
            (render_ai_topbar())
            (render_ai_main(contract_items))
        }
    }
}

fn render_ai_topbar() -> Markup {
    html! {
        header class="topbar" {
            (render_topbar_brand("Lince", None))
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
    }
}

fn render_ai_main(contract_items: &[String]) -> Markup {
    html! {
        main class="ai-lab" {
            (render_ai_hero())
            (render_ai_grid(contract_items))
        }
    }
}

fn render_ai_hero() -> Markup {
    html! {
        section class="ai-lab__hero" {
            div class="ai-lab__hero-copy" {
                div class="ai-lab__eyebrow" { "Experimental route" }
                h1 class="ai-lab__title" { "Criar widgets HTML com IA" }
                p class="ai-lab__description" {
                    "Descreva um widget, deixe o backend gerar o HTML completo com OpenAI, revise quantas vezes quiser e exporte o resultado como um arquivo pronto para arrastar de volta para o board."
                }
            }
            (render_ai_metrics())
        }
    }
}

fn render_ai_metrics() -> Markup {
    let metrics: [(&str, &str, &[&str]); 3] = [
        (
            "Modelo",
            "gpt-5.4-mini",
            &["Equilibrio entre custo e qualidade para iterar widgets."],
        ),
        (
            "Custo relativo",
            "1x",
            &["Geracao: ~8k-16k tokens", "Refinos: ~4k-9k tokens"],
        ),
        (
            "Seguranca",
            "API key em memoria",
            &["Some ao reiniciar o backend Rust."],
        ),
    ];

    html! {
        div class="ai-lab__hero-meta" {
            @for (label, value, hints) in metrics {
                (render_ai_metric_card(label, value, hints))
            }
        }
    }
}

fn render_ai_metric_card(label: &str, value: &str, hints: &[&str]) -> Markup {
    html! {
        div class="ai-metric-card" {
            span class="ai-metric-card__label" { (label) }
            strong class="ai-metric-card__value" { (value) }
            @for (index, hint) in hints.iter().enumerate() {
                @if index == 0 {
                    span class="ai-metric-card__hint" { (hint) }
                } @else {
                    span class="ai-metric-card__hint ai-metric-card__hint--secondary" { (hint) }
                }
            }
        }
    }
}

fn render_ai_grid(contract_items: &[String]) -> Markup {
    html! {
        section class="ai-lab__grid" {
            (render_composer_panel(contract_items))
            (render_preview_panel())
        }
    }
}

fn render_composer_panel(contract_items: &[String]) -> Markup {
    html! {
        section class="ai-panel ai-panel--composer" {
            (render_composer_header())
            (render_status_banner())
            (render_model_section())
            (render_error_banner())
            (render_generate_form())
            (render_contract_section(contract_items))
            (render_history_section())
        }
    }
}

fn render_composer_header() -> Markup {
    html! {
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
    }
}

fn render_status_banner() -> Markup {
    html! {
        div id="ai-status-banner" class="ai-status-banner" {
            span id="ai-status-pill" class="ai-status-banner__pill" { "Aguardando API key" }
            p id="ai-status-copy" class="ai-status-banner__copy" {
                "A chave vive apenas no backend desta sessao. A primeira geracao valida se ela realmente tem acesso ao modelo."
            }
        }
    }
}

fn render_model_section() -> Markup {
    html! {
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
    }
}

fn render_error_banner() -> Markup {
    html! {
        div id="ai-error-banner" class="ai-error-banner" hidden="" {}
    }
}

fn render_generate_form() -> Markup {
    html! {
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
                "O backend envia um preprompt forte com o contrato do widget HTML, regras visuais do Lince, limites de tamanho e orientacoes de microfrontend."
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
    }
}

fn render_contract_section(contract_items: &[String]) -> Markup {
    html! {
        section class="ai-contract" {
            div class="ai-contract__title" { "Contrato enviado para a IA" }
            ul class="ai-contract__list" {
                @for item in contract_items {
                    li { (item) }
                }
            }
        }
    }
}

fn render_history_section() -> Markup {
    html! {
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
}

fn render_preview_panel() -> Markup {
    html! {
        section class="ai-panel ai-panel--preview" {
            (render_preview_header())
            (render_preview_shell())
            (render_preview_meta())
        }
    }
}

fn render_preview_header() -> Markup {
    html! {
        header class="ai-panel__header ai-panel__header--preview" {
            div {
                div class="ai-panel__eyebrow" { "Preview" }
                h2 class="ai-panel__title" { "Resultado do widget HTML" }
            }
            a
                id="ai-export-link"
                class="modal-button modal-button--primary"
                href="#"
                hidden=""
                download=""
            {
                "Exportar HTML"
            }
        }
    }
}

fn render_preview_shell() -> Markup {
    html! {
        div class="ai-preview-shell" {
            (render_preview_toolbar())
            (render_preview_edit_controls())
            (render_preview_stage())
        }
    }
}

fn render_preview_toolbar() -> Markup {
    html! {
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
    }
}

fn render_preview_edit_controls() -> Markup {
    html! {
        div id="ai-preview-edit-controls" class="ai-preview-edit-controls" hidden="" {
            (render_dimension_control(
                "Largura",
                "ai-width-decrease",
                "ai-width-value",
                "ai-width-increase",
                "3",
                "Diminuir largura inicial",
                "Aumentar largura inicial",
            ))
            (render_dimension_control(
                "Altura",
                "ai-height-decrease",
                "ai-height-value",
                "ai-height-increase",
                "2",
                "Diminuir altura inicial",
                "Aumentar altura inicial",
            ))
            p class="ai-preview-edit-controls__hint" {
                "No modo editar o iframe fica travado para voce ajustar o tamanho inicial do widget antes de exportar."
            }
        }
    }
}

fn render_dimension_control(
    label: &str,
    decrease_id: &str,
    value_id: &str,
    increase_id: &str,
    value: &str,
    decrease_label: &str,
    increase_label: &str,
) -> Markup {
    html! {
        div class="ai-size-control" {
            span class="ai-size-control__label" { (label) }
            div class="ai-size-control__actions" {
                button id=(decrease_id) class="ai-size-control__button" type="button" aria-label=(decrease_label) { "-" }
                strong id=(value_id) class="ai-size-control__value" { (value) }
                button id=(increase_id) class="ai-size-control__button" type="button" aria-label=(increase_label) { "+" }
            }
        }
    }
}

fn render_preview_stage() -> Markup {
    html! {
        div id="ai-preview-stage" class="ai-preview-stage" style="--widget-cols:3;--widget-rows:2;" {
            div class="ai-preview-grid" aria-hidden="true" {
                @for _ in 0..36 {
                    div class="ai-preview-grid__cell" {}
                }
            }
            div id="ai-preview-empty" class="ai-preview-empty" {
                div class="ai-preview-empty__logo" aria-hidden="true" {
                    (render_lince_logo())
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
                    sandbox="allow-scripts"
                {}
                (render_preview_handle("e"))
                (render_preview_handle("s"))
                (render_preview_handle("se"))
            }
        }
    }
}

fn render_preview_handle(direction: &str) -> Markup {
    html! {
        button
            class=(format!("ai-preview-handle ai-preview-handle--{direction}"))
            type="button"
            tabindex="-1"
            aria-hidden="true"
            data-size-handle=(direction)
        {}
    }
}

fn render_preview_meta() -> Markup {
    html! {
        div class="ai-preview-meta" {
            (render_manifesto_meta())
            (render_permissions_meta())
            (render_manifest_preview_meta())
            (render_usage_meta())
        }
    }
}

fn render_manifesto_meta() -> Markup {
    html! {
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
    }
}

fn render_permissions_meta() -> Markup {
    html! {
        section class="ai-meta-card" {
            div class="ai-meta-card__eyebrow" { "Permissoes mock" }
            div id="ai-permissions-list" class="ai-permissions-list" {
                span class="ai-permissions-list__empty" { "Nenhuma permissao ainda." }
            }
        }
    }
}

fn render_manifest_preview_meta() -> Markup {
    html! {
        section class="ai-meta-card" {
            div class="ai-meta-card__eyebrow" { "Manifesto embutido" }
            pre id="ai-config-preview" class="ai-config-preview" { "{\n  \"title\": \"Widget title\",\n  \"author\": \"Author\"\n}" }
        }
    }
}

fn render_usage_meta() -> Markup {
    html! {
        section class="ai-meta-card" {
            div class="ai-meta-card__eyebrow" { "Uso da ultima geracao" }
            div id="ai-usage-summary" class="ai-usage-summary" { "Sem uso registrado ainda." }
        }
    }
}

fn render_ai_key_gate() -> Markup {
    html! {
        div id="ai-key-gate" class="ai-key-gate" {
            section class="ai-key-gate__panel" role="dialog" aria-modal="true" aria-labelledby="ai-key-gate-title" {
                div class="ai-key-gate__logo" aria-hidden="true" {
                    (render_lince_logo())
                }
                div class="ai-key-gate__eyebrow" { "OpenAI setup" }
                h2 id="ai-key-gate-title" class="ai-key-gate__title" { "Conecte sua API key para gerar widgets" }
                p class="ai-key-gate__copy" {
                    "A chave e enviada para o backend Rust e fica apenas em memoria nesta sessao. O host usa essa chave para pedir ao modelo um widget HTML completo com manifesto embutido em `lince-manifest`."
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
