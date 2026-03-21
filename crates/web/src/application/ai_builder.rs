use {
    crate::{
        domain::lince_package::{
            LincePackage, PackageManifest, build_lince_archive, normalize_html,
            normalize_permissions,
        },
        presentation::http::api_error::{ApiError, api_error},
    },
    axum::{
        Json,
        extract::{Path, State},
        http::{HeaderMap, HeaderValue, StatusCode, header},
        response::{IntoResponse, Response},
    },
    reqwest::Client,
    serde::{Deserialize, Serialize},
    serde_json::{Value, json},
    std::{collections::HashMap, sync::Arc},
    tokio::sync::RwLock,
    uuid::Uuid,
};

const OPENAI_RESPONSES_URL: &str = "https://api.openai.com/v1/responses";
const DEFAULT_AI_WIDGET_MODEL: &str = "gpt-5.4-mini";
const MAX_PROMPT_CHARS: usize = 8_000;
const MAX_OUTPUT_TOKENS: u32 = 10_000;
const WIDGET_SCHEMA_NAME: &str = "lince_widget_package";
const MODEL_PRICING_BASIS: &str = "Estimativa relativa baseada na tabela oficial de pricing do GPT-5.4, comparada ao gpt-5.4-mini.";
const WIDGET_BUILDER_SYSTEM_PROMPT: &str = r#"
You generate standalone Lince widgets.

Lince is a dark, minimal, premium widget board served by a Rust host app.
Each widget is exported as a `.lince` archive, which is just a zip renamed to `.lince`.
The archive contains exactly two files:
- index.html
- config.toml

Your job is to produce the contents needed for those files as a structured JSON object.

Hard requirements:
- Output valid JSON only, matching the requested schema.
- `html` must be a complete standalone document ready for `index.html`.
- The widget runs inside an iframe via `srcdoc`.
- The widget body is the card surface. Do not render an extra outer browser frame, modal, or "card inside card".
- Use inline CSS and inline JavaScript only. Do not depend on external CDNs, fonts, frameworks, or remote APIs.
- Do not assume network access.
- Keep the UI dark, minimal, technical, and solid-colored. Avoid gradients unless explicitly requested.
- Favor charcoal, graphite, muted grays, subtle borders, and restrained accent colors.
- Use negative space well and avoid template-looking layouts.
- The widget must feel native to the Lince host.
- The widget must support small card sizes and not overflow aggressively.
- The widget should remain useful even in preview mode.
- If you persist internal widget state, namespace localStorage with:
  `const instanceId = window.frameElement?.dataset?.packageInstanceId || "preview";`
- The host injects a bridge helper at `window.LinceWidgetHost`.
- If the widget wants reactive host state, add `data-lince-bridge-root` to a root element and listen with `data-on:lince-bridge-state`.
- The bridge event detail shape is `{ bridge, meta }`, so a Datastar widget can use `data-on:lince-bridge-state="$bridge = evt.detail.bridge"`.
- The widget may use SVG, canvas, or lightweight DOM manipulation.
- The widget should not call parent window APIs.
- Keep scripts simple and self-contained.

Manifest requirements:
- title: short user-facing widget name
- author: short author string
- version: semantic version string
- description: compact summary
- details: richer explanation of what the widget does
- initial_width: integer between 1 and 6
- initial_height: integer between 1 and 6
- permissions: mocked capabilities like `read_tasks`, `read_weather`, `control_playback`

Design guidance:
- Use a clean information hierarchy.
- Prefer strong typography contrast over decoration.
- Use subtle motion only when helpful.
- Avoid large paragraphs.
- Make controls tactile and compact.
- If the user asks for tables, lists, clocks, calendars, weather, music, notes, or dashboards, render them as polished microfrontends.

Behavior guidance:
- If the user asks to revise an existing widget, preserve the current working logic unless the request explicitly changes it.
- Keep generated JavaScript readable and bounded.
- Use accessible labels where relevant.
- Ensure the HTML can be zipped directly into a `.lince` file without further transformation.
"#;

#[derive(Clone)]
pub struct AiBuilderState {
    client: Client,
    api_key: Arc<RwLock<Option<String>>>,
    drafts: Arc<RwLock<HashMap<String, WidgetDraft>>>,
}

impl AiBuilderState {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("lince-frontend-web/0.1")
            .build()
            .expect("reqwest client should build");

        Self {
            client,
            api_key: Arc::new(RwLock::new(None)),
            drafts: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AiBuilderStatus {
    pub configured: bool,
    pub model: &'static str,
    pub models: Vec<AiModelOption>,
    pub create_token_estimate: &'static str,
    pub refine_token_estimate: &'static str,
    pub key_storage: &'static str,
    pub pricing_basis: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct AiModelOption {
    pub id: &'static str,
    pub name: &'static str,
    pub relative_price: &'static str,
    pub summary: &'static str,
    pub create_token_estimate: &'static str,
    pub refine_token_estimate: &'static str,
}

const AI_MODEL_CATALOG: [AiModelOption; 4] = [
    AiModelOption {
        id: "gpt-5.4-nano",
        name: "GPT-5.4 Nano",
        relative_price: "0.3x",
        summary: "Rascunhos rapidos, barato para explorar ideias e estrutura inicial do widget.",
        create_token_estimate: "6k-14k tokens",
        refine_token_estimate: "3k-7k tokens",
    },
    AiModelOption {
        id: "gpt-5.4-mini",
        name: "GPT-5.4 Mini",
        relative_price: "1x",
        summary: "Melhor equilibrio entre custo, consistencia visual e iteracao para a maioria dos widgets.",
        create_token_estimate: "8k-16k tokens",
        refine_token_estimate: "4k-9k tokens",
    },
    AiModelOption {
        id: "gpt-5.4",
        name: "GPT-5.4",
        relative_price: "3.3x",
        summary: "Mais criterio de design e melhor capacidade de refino quando o widget fica complexo.",
        create_token_estimate: "8k-18k tokens",
        refine_token_estimate: "4k-10k tokens",
    },
    AiModelOption {
        id: "gpt-5.4-pro",
        name: "GPT-5.4 Pro",
        relative_price: "40x",
        summary: "Qualidade maxima e custo alto. Use quando quiser insistir num widget mais sofisticado.",
        create_token_estimate: "10k-20k tokens",
        refine_token_estimate: "5k-12k tokens",
    },
];

#[derive(Debug, Deserialize)]
pub struct StoreApiKeyRequest {
    pub api_key: String,
}

#[derive(Debug, Deserialize)]
pub struct GenerateWidgetRequest {
    pub prompt: String,
    pub draft_id: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDraftSizeRequest {
    pub initial_width: u8,
    pub initial_height: u8,
}

#[derive(Debug, Clone)]
struct WidgetDraft {
    id: String,
    package: LincePackage,
    source_prompt: String,
    revision: u32,
    usage: UsageSummary,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct UsageSummary {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct GenerateWidgetResponse {
    pub draft: WidgetDraftPreview,
}

#[derive(Debug, Serialize)]
pub struct WidgetDraftPreview {
    pub id: String,
    pub title: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub details: String,
    pub initial_width: u8,
    pub initial_height: u8,
    pub permissions: Vec<String>,
    pub html: String,
    pub source_prompt: String,
    pub revision: u32,
    pub usage: UsageSummary,
    pub config_toml: String,
    pub download_url: String,
    pub filename: String,
}

impl From<&WidgetDraft> for WidgetDraftPreview {
    fn from(draft: &WidgetDraft) -> Self {
        let manifest = &draft.package.manifest;
        let filename = draft.package.archive_filename();
        Self {
            id: draft.id.clone(),
            title: manifest.title.clone(),
            author: manifest.author.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            details: manifest.details.clone(),
            initial_width: manifest.initial_width,
            initial_height: manifest.initial_height,
            permissions: manifest.permissions.clone(),
            html: draft.package.html.clone(),
            source_prompt: draft.source_prompt.clone(),
            revision: draft.revision,
            usage: draft.usage.clone(),
            config_toml: draft.package.config_toml(),
            download_url: format!("/api/ai/drafts/{}/download", draft.id),
            filename,
        }
    }
}

#[derive(Debug, Deserialize)]
struct GeneratedWidgetPayload {
    title: String,
    author: String,
    version: String,
    description: String,
    details: String,
    initial_width: u8,
    initial_height: u8,
    permissions: Vec<String>,
    html: String,
}

pub async fn ai_builder_status(State(state): State<AiBuilderState>) -> Json<AiBuilderStatus> {
    let configured = state.api_key.read().await.is_some();
    Json(status_payload(configured))
}

pub async fn store_api_key(
    State(state): State<AiBuilderState>,
    Json(request): Json<StoreApiKeyRequest>,
) -> Result<Json<AiBuilderStatus>, (StatusCode, Json<ApiError>)> {
    let api_key = request.api_key.trim();
    if api_key.is_empty() {
        return Err(client_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "A API key nao pode estar vazia.",
        ));
    }

    *state.api_key.write().await = Some(api_key.to_string());

    Ok(Json(status_payload(true)))
}

pub async fn generate_widget(
    State(state): State<AiBuilderState>,
    Json(request): Json<GenerateWidgetRequest>,
) -> Result<Json<GenerateWidgetResponse>, (StatusCode, Json<ApiError>)> {
    let prompt = request.prompt.trim();
    if prompt.is_empty() {
        return Err(client_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Descreva o widget antes de pedir a geracao.",
        ));
    }

    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(client_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "O prompt ficou grande demais para esta demo experimental.",
        ));
    }

    let api_key = state.api_key.read().await.clone().ok_or_else(|| {
        client_error(
            StatusCode::UNAUTHORIZED,
            "Cadastre uma API key da OpenAI antes de gerar um widget.",
        )
    })?;
    let selected_model = resolve_model(request.model.as_deref())
        .map_err(|message| client_error(StatusCode::UNPROCESSABLE_ENTITY, &message))?;

    let existing_draft = if let Some(draft_id) = request.draft_id.as_deref() {
        state
            .drafts
            .read()
            .await
            .get(draft_id)
            .cloned()
            .ok_or_else(|| {
                client_error(
                    StatusCode::NOT_FOUND,
                    "Nao encontrei o draft atual para aplicar a modificacao.",
                )
            })?
            .into()
    } else {
        None
    };

    let (generated, usage) = request_openai_widget(
        &state.client,
        &api_key,
        selected_model.id,
        prompt,
        existing_draft.as_ref(),
    )
    .await
    .map_err(openai_error)?;

    let mut draft = normalize_generated_widget(generated, prompt, existing_draft.as_ref())
        .map_err(invalid_generated_widget)?;
    draft.usage = usage;
    let preview = WidgetDraftPreview::from(&draft);
    let draft_id = draft.id.clone();

    state.drafts.write().await.insert(draft_id, draft);

    Ok(Json(GenerateWidgetResponse { draft: preview }))
}

pub async fn update_draft_size(
    Path(draft_id): Path<String>,
    State(state): State<AiBuilderState>,
    Json(request): Json<UpdateDraftSizeRequest>,
) -> Result<Json<GenerateWidgetResponse>, (StatusCode, Json<ApiError>)> {
    validate_dimension("Largura", request.initial_width)?;
    validate_dimension("Altura", request.initial_height)?;

    let mut drafts = state.drafts.write().await;
    let draft = drafts.get_mut(&draft_id).ok_or_else(|| {
        client_error(
            StatusCode::NOT_FOUND,
            "Esse draft nao existe mais na sessao atual.",
        )
    })?;

    draft.package.manifest.initial_width = request.initial_width;
    draft.package.manifest.initial_height = request.initial_height;

    Ok(Json(GenerateWidgetResponse {
        draft: WidgetDraftPreview::from(&*draft),
    }))
}

pub async fn download_draft(
    Path(draft_id): Path<String>,
    State(state): State<AiBuilderState>,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    let draft = state
        .drafts
        .read()
        .await
        .get(&draft_id)
        .cloned()
        .ok_or_else(|| {
            client_error(
                StatusCode::NOT_FOUND,
                "Esse draft nao existe mais na sessao atual.",
            )
        })?;

    let archive = build_lince_archive(&draft.package).map_err(internal_error)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"{}\"",
            draft.package.archive_filename()
        ))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );

    Ok((headers, archive).into_response())
}

impl WidgetDraft {}

fn status_payload(configured: bool) -> AiBuilderStatus {
    let default_model = default_model();
    AiBuilderStatus {
        configured,
        model: default_model.id,
        models: AI_MODEL_CATALOG.to_vec(),
        create_token_estimate: default_model.create_token_estimate,
        refine_token_estimate: default_model.refine_token_estimate,
        key_storage: "Backend memory only. The key is lost when the Rust server restarts.",
        pricing_basis: MODEL_PRICING_BASIS,
    }
}

fn normalize_generated_widget(
    generated: GeneratedWidgetPayload,
    prompt: &str,
    existing: Option<&WidgetDraft>,
) -> Result<WidgetDraft, String> {
    let draft_id = existing
        .map(|draft| draft.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let title = fallback_string(generated.title, "Untitled widget");
    let author = fallback_string(generated.author, "Lince AI Lab");
    let version = fallback_string(generated.version, "0.1.0");
    let description = fallback_string(
        generated.description,
        "Widget gerado por IA para a board experimental do Lince.",
    );
    let details = fallback_string(
        generated.details,
        "Widget .lince gerado de forma experimental a partir de um prompt em linguagem natural.",
    );

    let package = LincePackage::new(
        None,
        PackageManifest {
            icon: "◧".into(),
            title,
            author,
            version,
            description,
            details,
            initial_width: generated.initial_width.clamp(1, 6),
            initial_height: generated.initial_height.clamp(1, 6),
            permissions: normalize_permissions(generated.permissions),
        },
        normalize_html(&generated.html)?,
    )?;

    Ok(WidgetDraft {
        id: draft_id,
        package,
        source_prompt: prompt.to_string(),
        revision: existing.map_or(1, |draft| draft.revision + 1),
        usage: UsageSummary::default(),
    })
}

async fn request_openai_widget(
    client: &Client,
    api_key: &str,
    model: &str,
    prompt: &str,
    existing: Option<&WidgetDraft>,
) -> Result<(GeneratedWidgetPayload, UsageSummary), String> {
    let body = json!({
        "model": model,
        "input": [
            {
                "role": "system",
                "content": [
                    {
                        "type": "input_text",
                        "text": WIDGET_BUILDER_SYSTEM_PROMPT,
                    }
                ]
            },
            {
                "role": "user",
                "content": [
                    {
                        "type": "input_text",
                        "text": build_user_prompt(prompt, existing),
                    }
                ]
            }
        ],
        "text": {
            "format": {
                "type": "json_schema",
                "name": WIDGET_SCHEMA_NAME,
                "strict": true,
                "schema": widget_response_schema(),
            }
        },
        "max_output_tokens": MAX_OUTPUT_TOKENS,
    });

    let response = client
        .post(OPENAI_RESPONSES_URL)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("Falha ao falar com a OpenAI: {error}"))?;

    let status = response.status();
    let response_text = response
        .text()
        .await
        .map_err(|error| format!("Falha ao ler a resposta da OpenAI: {error}"))?;

    if !status.is_success() {
        return Err(extract_openai_error(&response_text)
            .unwrap_or_else(|| format!("A OpenAI recusou a geracao ({status}).")));
    }

    let response_json: Value = serde_json::from_str(&response_text)
        .map_err(|error| format!("A resposta da OpenAI nao veio em JSON valido: {error}"))?;
    let output_text = extract_output_text(&response_json)
        .ok_or_else(|| "A OpenAI nao devolveu o JSON do widget.".to_string())?;
    let generated: GeneratedWidgetPayload = serde_json::from_str(&output_text)
        .map_err(|error| format!("Nao consegui validar o widget retornado pela OpenAI: {error}"))?;

    Ok((generated, parse_usage(&response_json)))
}

fn build_user_prompt(prompt: &str, existing: Option<&WidgetDraft>) -> String {
    let mut message = String::from("Create or revise a Lince widget package.\n\n");

    if let Some(draft) = existing {
        message.push_str("Current draft manifest:\n");
        message.push_str(&format!(
            "- title: {}\n- author: {}\n- version: {}\n- description: {}\n- details: {}\n- initial_width: {}\n- initial_height: {}\n- permissions: {}\n\n",
            draft.package.manifest.title,
            draft.package.manifest.author,
            draft.package.manifest.version,
            draft.package.manifest.description,
            draft.package.manifest.details,
            draft.package.manifest.initial_width,
            draft.package.manifest.initial_height,
            draft.package.manifest.permissions.join(", "),
        ));
        message.push_str("Current index.html:\n```html\n");
        message.push_str(&draft.package.html);
        message.push_str("\n```\n\n");
        message.push_str("Revise the current widget according to the next request.\n\n");
    } else {
        message.push_str("Create a brand new widget from scratch.\n\n");
    }

    message.push_str("User request:\n");
    message.push_str(prompt);

    message
}

fn widget_response_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "title",
            "author",
            "version",
            "description",
            "details",
            "initial_width",
            "initial_height",
            "permissions",
            "html"
        ],
        "properties": {
            "title": {
                "type": "string",
                "minLength": 1,
                "maxLength": 80
            },
            "author": {
                "type": "string",
                "minLength": 1,
                "maxLength": 80
            },
            "version": {
                "type": "string",
                "minLength": 1,
                "maxLength": 24
            },
            "description": {
                "type": "string",
                "minLength": 1,
                "maxLength": 160
            },
            "details": {
                "type": "string",
                "minLength": 1,
                "maxLength": 900
            },
            "initial_width": {
                "type": "integer",
                "minimum": 1,
                "maximum": 6
            },
            "initial_height": {
                "type": "integer",
                "minimum": 1,
                "maximum": 6
            },
            "permissions": {
                "type": "array",
                "maxItems": 8,
                "items": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 64
                }
            },
            "html": {
                "type": "string",
                "minLength": 1
            }
        }
    })
}

fn extract_openai_error(raw_body: &str) -> Option<String> {
    let json = serde_json::from_str::<Value>(raw_body).ok()?;
    json.get("error")?
        .get("message")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn extract_output_text(response: &Value) -> Option<String> {
    if let Some(text) = response.get("output_text").and_then(Value::as_str) {
        return Some(text.to_string());
    }

    let mut parts = Vec::new();
    let output = response.get("output")?.as_array()?;
    for item in output {
        let Some(content_parts) = item.get("content").and_then(Value::as_array) else {
            continue;
        };

        for part in content_parts {
            if part.get("type").and_then(Value::as_str) != Some("output_text") {
                continue;
            }

            if let Some(text) = part.get("text").and_then(Value::as_str) {
                parts.push(text.to_string());
            }
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

fn parse_usage(response: &Value) -> UsageSummary {
    let usage = response.get("usage");
    UsageSummary {
        input_tokens: usage
            .and_then(|value| value.get("input_tokens"))
            .and_then(Value::as_u64)
            .map(|value| value as u32),
        output_tokens: usage
            .and_then(|value| value.get("output_tokens"))
            .and_then(Value::as_u64)
            .map(|value| value as u32),
        total_tokens: usage
            .and_then(|value| value.get("total_tokens"))
            .and_then(Value::as_u64)
            .map(|value| value as u32),
    }
}

fn fallback_string(value: String, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn default_model() -> &'static AiModelOption {
    AI_MODEL_CATALOG
        .iter()
        .find(|model| model.id == DEFAULT_AI_WIDGET_MODEL)
        .unwrap_or(&AI_MODEL_CATALOG[1])
}

fn resolve_model(model_id: Option<&str>) -> Result<&'static AiModelOption, String> {
    let selected = model_id.unwrap_or(DEFAULT_AI_WIDGET_MODEL);
    AI_MODEL_CATALOG
        .iter()
        .find(|model| model.id == selected)
        .ok_or_else(|| "Esse modelo nao esta disponivel nesta demo.".to_string())
}

fn validate_dimension(label: &str, value: u8) -> Result<(), (StatusCode, Json<ApiError>)> {
    if (1..=6).contains(&value) {
        Ok(())
    } else {
        Err(client_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            &format!("{label} inicial precisa ficar entre 1 e 6 celulas."),
        ))
    }
}

fn client_error(status: StatusCode, message: &str) -> (StatusCode, Json<ApiError>) {
    api_error(status, message)
}

fn openai_error(message: String) -> (StatusCode, Json<ApiError>) {
    client_error(StatusCode::BAD_GATEWAY, &message)
}

fn internal_error(message: String) -> (StatusCode, Json<ApiError>) {
    client_error(StatusCode::INTERNAL_SERVER_ERROR, &message)
}

fn invalid_generated_widget(message: String) -> (StatusCode, Json<ApiError>) {
    client_error(StatusCode::UNPROCESSABLE_ENTITY, &message)
}
