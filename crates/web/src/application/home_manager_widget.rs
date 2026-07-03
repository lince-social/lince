use {
    crate::{
        application::{
            backend_api::BackendApiService,
            home_manager_identity::is_supported_home_manager_package_filename,
        },
        domain::board::BoardState,
        infrastructure::board_state_store::BoardStateStore,
    },
    ::application::auth::AuthSubject,
    serde::{Deserialize, Serialize},
    serde_json::{Map, Value, json},
};

const ALIMENTUM_NAMESPACE: &str = "nutrition.alimentum.v1";

#[derive(Clone)]
pub struct HomeManagerWidgetService {
    backend: BackendApiService,
    board_state: BoardStateStore,
}

impl HomeManagerWidgetService {
    pub fn new(backend: BackendApiService, board_state: BoardStateStore) -> Self {
        Self {
            backend,
            board_state,
        }
    }

    pub async fn action(
        &self,
        instance_id: &str,
        action: &str,
        payload: Value,
    ) -> Result<Value, HomeManagerWidgetError> {
        self.ensure_home_manager(instance_id).await?;
        match action {
            "home-manager-list-alimenta" => self.list_alimenta().await,
            "home-manager-create-alimentum" => {
                let request = serde_json::from_value::<SaveAlimentumRequest>(payload)
                    .map_err(|error| HomeManagerWidgetError::Invalid(error.to_string()))?;
                self.create_alimentum(request).await
            }
            "home-manager-update-alimentum" => {
                let request = serde_json::from_value::<SaveAlimentumRequest>(payload)
                    .map_err(|error| HomeManagerWidgetError::Invalid(error.to_string()))?;
                self.update_alimentum(request).await
            }
            _ => Err(HomeManagerWidgetError::Invalid(format!(
                "Unsupported Home Manager action: {action}"
            ))),
        }
    }

    async fn ensure_home_manager(&self, instance_id: &str) -> Result<(), HomeManagerWidgetError> {
        let board_state = self.board_state.snapshot().await;
        let card = find_board_card(&board_state, instance_id)
            .ok_or_else(|| HomeManagerWidgetError::NotFound("Widget not found.".into()))?;
        if !is_supported_home_manager_package_filename(&card.package_name) {
            return Err(HomeManagerWidgetError::Invalid(
                "Widget is not Home Manager.".into(),
            ));
        }
        Ok(())
    }

    async fn list_alimenta(&self) -> Result<Value, HomeManagerWidgetError> {
        let records = serde_json::from_value::<Vec<RecordRow>>(
            self.backend
                .list_table_rows(&local_host_subject(), "record")
                .await
                .map_err(map_backend_error)?,
        )
        .map_err(|error| HomeManagerWidgetError::Internal(error.to_string()))?;
        let extensions = serde_json::from_value::<Vec<RecordExtensionRow>>(
            self.backend
                .list_table_rows(&local_host_subject(), "record_extension")
                .await
                .map_err(map_backend_error)?,
        )
        .map_err(|error| HomeManagerWidgetError::Internal(error.to_string()))?;

        let mut items = Vec::new();
        for extension in extensions {
            if extension.namespace != ALIMENTUM_NAMESPACE {
                continue;
            }
            let Some(record) = records.iter().find(|row| row.id == extension.record_id) else {
                continue;
            };
            let mut value = serde_json::from_str::<Value>(&extension.freestyle_data_structure)
                .map_err(|error| HomeManagerWidgetError::Internal(error.to_string()))?;
            if let Some(food) = value.get_mut("food").and_then(Value::as_object_mut) {
                food.insert("recordId".into(), json!(record.id));
                food.insert("extensionId".into(), json!(extension.id));
                food.entry("name")
                    .or_insert_with(|| json!(record.head.clone().unwrap_or_default()));
            }
            items.push(json!({
                "recordId": record.id,
                "extensionId": extension.id,
                "head": record.head,
                "body": record.body,
                "extension": value,
            }));
        }

        Ok(json!({ "items": items }))
    }

    async fn create_alimentum(
        &self,
        request: SaveAlimentumRequest,
    ) -> Result<Value, HomeManagerWidgetError> {
        let food = normalize_food_payload(request.food)?;
        let name = food
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Alimentum")
            .trim()
            .to_string();

        let mut record_payload = Map::new();
        record_payload.insert("quantity".into(), json!(0));
        record_payload.insert("head".into(), json!(name));
        record_payload.insert(
            "body".into(),
            request.body.map(Value::String).unwrap_or(Value::Null),
        );
        let created = self
            .backend
            .create_table_row(&local_host_subject(), "record", &record_payload)
            .await
            .map_err(map_backend_error)?;
        let record_id = created.last_insert_rowid.ok_or_else(|| {
            HomeManagerWidgetError::Internal("Record creation did not return an id.".into())
        })?;

        let mut extension_payload = Map::new();
        extension_payload.insert("record_id".into(), json!(record_id));
        extension_payload.insert("namespace".into(), json!(ALIMENTUM_NAMESPACE));
        extension_payload.insert("version".into(), json!(1));
        extension_payload.insert(
            "freestyle_data_structure".into(),
            json!(json!({ "schema": ALIMENTUM_NAMESPACE, "food": food }).to_string()),
        );
        let extension = self
            .backend
            .create_table_row(
                &local_host_subject(),
                "record_extension",
                &extension_payload,
            )
            .await
            .map_err(map_backend_error)?;

        Ok(json!({
            "ok": true,
            "recordId": record_id,
            "extensionId": extension.last_insert_rowid,
        }))
    }

    async fn update_alimentum(
        &self,
        request: SaveAlimentumRequest,
    ) -> Result<Value, HomeManagerWidgetError> {
        let record_id = request
            .record_id
            .ok_or_else(|| HomeManagerWidgetError::Invalid("recordId is required.".into()))?;
        let extension_id = request
            .extension_id
            .ok_or_else(|| HomeManagerWidgetError::Invalid("extensionId is required.".into()))?;
        let food = normalize_food_payload(request.food)?;
        let name = food
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Alimentum")
            .trim()
            .to_string();

        let mut record_payload = Map::new();
        record_payload.insert("head".into(), json!(name));
        if let Some(body) = request.body {
            record_payload.insert("body".into(), json!(body));
        }
        self.backend
            .update_table_row(&local_host_subject(), "record", record_id, &record_payload)
            .await
            .map_err(map_backend_error)?;

        let mut extension_payload = Map::new();
        extension_payload.insert(
            "freestyle_data_structure".into(),
            json!(json!({ "schema": ALIMENTUM_NAMESPACE, "food": food }).to_string()),
        );
        self.backend
            .update_table_row(
                &local_host_subject(),
                "record_extension",
                extension_id,
                &extension_payload,
            )
            .await
            .map_err(map_backend_error)?;

        Ok(json!({ "ok": true, "recordId": record_id, "extensionId": extension_id }))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveAlimentumRequest {
    record_id: Option<i64>,
    extension_id: Option<i64>,
    body: Option<String>,
    food: Value,
}

#[derive(Debug, Deserialize)]
struct RecordRow {
    id: i64,
    head: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecordExtensionRow {
    id: i64,
    record_id: i64,
    namespace: String,
    freestyle_data_structure: String,
}

#[derive(Debug, Serialize)]
pub enum HomeManagerWidgetError {
    NotFound(String),
    Invalid(String),
    Internal(String),
}

fn normalize_food_payload(value: Value) -> Result<Value, HomeManagerWidgetError> {
    let object = value
        .as_object()
        .ok_or_else(|| HomeManagerWidgetError::Invalid("food must be an object.".into()))?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if name.is_empty() {
        return Err(HomeManagerWidgetError::Invalid(
            "food.name is required.".into(),
        ));
    }
    if !object
        .get("nutrients")
        .is_some_and(|nutrients| nutrients.as_object().is_some())
    {
        return Err(HomeManagerWidgetError::Invalid(
            "food.nutrients is required.".into(),
        ));
    }
    Ok(value)
}

fn find_board_card(
    board_state: &BoardState,
    instance_id: &str,
) -> Option<crate::domain::board::BoardCard> {
    board_state
        .workspaces
        .iter()
        .flat_map(|workspace| workspace.cards.iter())
        .find(|card| card.id == instance_id)
        .cloned()
}

fn local_host_subject() -> AuthSubject {
    AuthSubject::system()
}

fn map_backend_error(error: std::io::Error) -> HomeManagerWidgetError {
    HomeManagerWidgetError::Internal(error.to_string())
}
