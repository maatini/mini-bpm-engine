use crate::server::state::{AppError, AppState, parse_uuid};
use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) async fn upload_instance_file(
    State(state): State<Arc<AppState>>,
    Path((id, var_name)): Path<(String, String)>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let engine = &state.engine;
    let instance_id = parse_uuid(&id)?;
    if engine.get_instance_details(instance_id).await.is_err() {
        return Err(AppError::BadRequest("Instance not found".into()));
    }

    if let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        let filename = field.file_name().unwrap_or("unknown").to_string();
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?;

        let file_ref = engine_core::model::FileReference::new(
            instance_id,
            &var_name,
            &filename,
            &content_type,
            data.len() as u64,
        );

        if let Some(persistence) = &state.persistence {
            persistence
                .save_file(&file_ref.object_key, &data)
                .await
                .map_err(|e| AppError::BadRequest(format!("Failed to save file: {:?}", e)))?;
        }

        let mut vars = HashMap::new();
        vars.insert(var_name, file_ref.to_variable_value());
        engine.update_instance_variables(instance_id, vars).await?;

        Ok(StatusCode::CREATED)
    } else {
        Err(AppError::BadRequest("No file field provided".into()))
    }
}

pub(crate) async fn get_instance_file(
    State(state): State<Arc<AppState>>,
    Path((id, var_name)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let engine = &state.engine;
    let instance_id = parse_uuid(&id)?;
    let instance = engine.get_instance_details(instance_id).await?;

    let file_ref = instance
        .get_file_reference(&var_name)
        .ok_or_else(|| AppError::BadRequest("Variable is not a file".into()))?;

    if let Some(persistence) = &state.persistence {
        let data = persistence
            .load_file(&file_ref.object_key)
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to load file: {:?}", e)))?;

        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            file_ref
                .mime_type
                .parse()
                .unwrap_or(axum::http::HeaderValue::from_static(
                    "application/octet-stream",
                )),
        );
        headers.insert(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", file_ref.filename)
                .parse()
                .unwrap_or(axum::http::HeaderValue::from_static("attachment")),
        );

        Ok((headers, data))
    } else {
        Err(AppError::BadRequest("No persistence configured".into()))
    }
}

pub(crate) async fn delete_instance_file(
    State(state): State<Arc<AppState>>,
    Path((id, var_name)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let engine = &state.engine;
    let instance_id = parse_uuid(&id)?;
    let instance = engine.get_instance_details(instance_id).await?;

    let file_ref = instance
        .get_file_reference(&var_name)
        .ok_or_else(|| AppError::BadRequest("Variable is not a file".into()))?;

    if let Some(persistence) = &state.persistence {
        persistence
            .delete_file(&file_ref.object_key)
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to delete file: {:?}", e)))?;
    }

    let mut vars = HashMap::new();
    vars.insert(var_name, Value::Null);
    engine.update_instance_variables(instance_id, vars).await?;

    Ok(StatusCode::NO_CONTENT)
}
