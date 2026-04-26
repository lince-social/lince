use axum::{Json, response::Html};
use utoipa::OpenApi;

use crate::presentation::http::api::backend::{
    __path_batch_update_record_quantities, __path_create_karma_row, __path_create_table_row,
    __path_delete_karma_row, __path_delete_link, __path_delete_table_row, __path_delete_via_link,
    __path_download_link, __path_download_via_link, __path_execute_karma, __path_get_karma_row,
    __path_get_table_row, __path_list_files, __path_list_karma_rows, __path_list_table_rows,
    __path_login, __path_update_karma_row, __path_update_table_row, __path_update_table_rows,
    __path_upload_link, __path_upload_via_link, __path_view_snapshot, __path_view_sse,
};
use crate::presentation::http::api::backend::{
    FileKeyRequest, FileLinkResponse, LoginRequest, LoginResponse, MutationResponse,
};
use crate::presentation::http::api::servers::{
    __path_create_server, __path_delete_server, __path_list_servers, __path_login_server,
    __path_logout_server, __path_update_server, ServerLoginRequest, ServerProfileResponse,
    UpsertServerProfileRequest,
};

use crate::presentation::http::api_error::ApiError;

#[derive(OpenApi)]
#[openapi(
    paths(
        login,
        list_table_rows,
        get_table_row,
        create_table_row,
        update_table_rows,
        update_table_row,
        delete_table_row,
        batch_update_record_quantities,
        list_karma_rows,
        get_karma_row,
        create_karma_row,
        update_karma_row,
        delete_karma_row,
        execute_karma,
        list_files,
        upload_link,
        download_link,
        delete_link,
        upload_via_link,
        download_via_link,
        delete_via_link,
        view_snapshot,
        view_sse,
        list_servers,
        create_server,
        update_server,
        delete_server,
        login_server,
        logout_server
    ),
    components(schemas(
        ApiError,
        FileKeyRequest,
        FileLinkResponse,
        LoginRequest,
        LoginResponse,
        MutationResponse,
        ServerLoginRequest,
        ServerProfileResponse,
        UpsertServerProfileRequest
    )),
    tags(
        (name = "auth", description = "Authentication endpoints"),
        (name = "table", description = "Generic table endpoints"),
        (name = "karma", description = "Karma table endpoints"),
        (name = "files", description = "File and access link endpoints"),
        (name = "view", description = "View snapshot and streaming endpoints"),
        (name = "organ", description = "Organ management endpoints")
    )
)]
pub struct ApiDoc;

pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

pub async fn swagger_ui() -> Html<&'static str> {
    Html(SWAGGER_UI_HTML)
}

const SWAGGER_UI_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Lince API Docs</title>
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui.css" />
  <style>
    body {
      margin: 0;
      background: #0b0f14;
    }
    .swagger-ui {
      background: #0b0f14;
      color: #e5eef8;
      min-height: 100vh;
    }
    .swagger-ui .topbar {
      background: #0f1520;
      border-bottom: 1px solid #233044;
    }
    .swagger-ui .info .title,
    .swagger-ui .info p,
    .swagger-ui .opblock-summary-description,
    .swagger-ui .opblock-description-wrapper p,
    .swagger-ui .opblock-section-header h4,
    .swagger-ui .parameter__name,
    .swagger-ui .response-col_status,
    .swagger-ui .response-col_description,
    .swagger-ui .response-col_links,
    .swagger-ui .response-col_media-type,
    .swagger-ui .response-col_description__inner,
    .swagger-ui .renderedMarkdown,
    .swagger-ui .opblock-tag {
      color: #e5eef8;
    }
    .swagger-ui .scheme-container,
    .swagger-ui .opblock,
    .swagger-ui .model-box,
    .swagger-ui .model,
    .swagger-ui .highlight-code,
    .swagger-ui .parameters-container,
    .swagger-ui .responses-wrapper,
    .swagger-ui .opblock-section-header,
    .swagger-ui .tab-panel,
    .swagger-ui .info,
    .swagger-ui .parameter__name,
    .swagger-ui .opblock-summary,
    .swagger-ui .btn,
    .swagger-ui input,
    .swagger-ui select,
    .swagger-ui textarea {
      background: #121923;
      border-color: #2a3748;
    }
    .swagger-ui .opblock-tag-section {
      border-bottom-color: #233044;
    }
    .swagger-ui .opblock.opblock-get .opblock-summary-method { background: #38bdf8; }
    .swagger-ui .opblock.opblock-post .opblock-summary-method { background: #34d399; }
    .swagger-ui .opblock.opblock-patch .opblock-summary-method { background: #f59e0b; }
    .swagger-ui .opblock.opblock-delete .opblock-summary-method { background: #f87171; }
    .swagger-ui .opblock.opblock-put .opblock-summary-method { background: #a78bfa; }
    .swagger-ui .opblock .opblock-summary {
      border-bottom-color: #2a3748;
    }
  </style>
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui-standalone-preset.js"></script>
  <script>
    window.onload = function() {
      window.ui = SwaggerUIBundle({
        url: "/openapi.json",
        dom_id: "#swagger-ui",
        deepLinking: true,
        filter: true,
        displayOperationId: true,
        docExpansion: "list",
        defaultModelsExpandDepth: -1,
        tagsSorter: "alpha",
        operationsSorter: "alpha",
        showExtensions: true,
        presets: [
          SwaggerUIBundle.presets.apis,
          SwaggerUIStandalonePreset
        ],
        layout: "StandaloneLayout"
      });
    };
  </script>
</body>
</html>
"##;
