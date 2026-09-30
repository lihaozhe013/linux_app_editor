use std::path::PathBuf;

use serde::Deserialize;
use systemd_service_core::templates::ServiceTemplate;
use systemd_service_core::unit_text::{self, FormProjection, ServiceForm};
use systemd_service_core::{
    self as service_core, Diagnostic, SaveOutcome, Scope, ServiceDocument, ServiceItem,
    VerifyOutcome,
};

fn scope(value: &str) -> Result<Scope, String> {
    match value {
        "user" => Ok(Scope::User),
        "system" => Ok(Scope::System),
        _ => Err(format!("unknown service scope: {value}")),
    }
}

#[tauri::command]
pub fn list_systemd_services(scope_name: String) -> Result<Vec<ServiceItem>, String> {
    service_core::list_services(scope(&scope_name)?).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_systemd_service(path: String, scope_name: String) -> Result<ServiceDocument, String> {
    service_core::open_service(&PathBuf::from(path), scope(&scope_name)?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_systemd_service(
    scope_name: String,
    unit_name: String,
) -> Result<ServiceDocument, String> {
    service_core::create_service(scope(&scope_name)?, &unit_name).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_systemd_service_templates() -> Vec<ServiceTemplate> {
    service_core::templates::templates()
}

#[tauri::command]
pub fn create_systemd_service_from_template(
    template_id: String,
    unit_name: String,
    exec_path: String,
) -> Result<ServiceDocument, String> {
    service_core::create_service_from_template(&template_id, &unit_name, &exec_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn suggest_systemd_unit_name(exec_path: String) -> String {
    service_core::templates::suggested_unit_name(&exec_path)
}

#[tauri::command]
pub fn project_systemd_form(contents: String, is_drop_in: bool) -> FormProjection {
    unit_text::project_form(&contents, is_drop_in)
}

#[tauri::command]
pub fn apply_systemd_form(
    contents: String,
    form: ServiceForm,
    dirty_fields: Vec<String>,
    is_drop_in: bool,
) -> Result<String, String> {
    unit_text::apply_form(&contents, &form, &dirty_fields, is_drop_in)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn validate_systemd_draft(contents: String, is_drop_in: bool) -> Vec<Diagnostic> {
    service_core::validate(&contents, is_drop_in)
}

#[tauri::command]
pub fn preview_systemd_diff(before: String, after: String) -> String {
    service_core::diff(&before, &after)
}

#[derive(Debug, Deserialize)]
pub struct SaveSystemdServiceRequest {
    pub document: ServiceDocument,
    pub contents: String,
    #[serde(default)]
    pub make_backup: bool,
    #[serde(default)]
    pub stage_system: bool,
}

#[tauri::command]
pub fn save_systemd_service(req: SaveSystemdServiceRequest) -> Result<SaveOutcome, String> {
    service_core::save_service(
        &req.document,
        &req.contents,
        req.make_backup,
        req.stage_system,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn verify_systemd_service(path: String, scope_name: String) -> Result<VerifyOutcome, String> {
    service_core::verify_service(&PathBuf::from(path), scope(&scope_name)?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reload_systemd(scope_name: String) -> Result<String, String> {
    service_core::reload(scope(&scope_name)?).map_err(|error| error.to_string())
}
