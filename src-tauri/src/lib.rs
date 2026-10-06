#[cfg(test)]
pub mod env_lock;

pub mod appimage;
pub mod commands;
pub mod desktop_entry;
pub mod error;
pub mod filesystem;
pub mod icons;
pub mod locations;
pub mod service_commands;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::create_launcher,
            commands::extract_appimage_metadata,
            commands::open_desktop_entry,
            commands::save_desktop_entry,
            commands::delete_launcher,
            commands::list_managed_launchers,
            commands::list_desktop_locations,
            commands::validate_form,
            commands::run_desktop_file_validate,
            commands::find_nearby_icons,
            commands::stat_path,
            service_commands::list_systemd_services,
            service_commands::open_systemd_service,
            service_commands::create_systemd_service,
            service_commands::list_systemd_service_templates,
            service_commands::create_systemd_service_from_template,
            service_commands::suggest_systemd_unit_name,
            service_commands::project_systemd_form,
            service_commands::apply_systemd_form,
            service_commands::validate_systemd_draft,
            service_commands::preview_systemd_diff,
            service_commands::save_systemd_service,
            service_commands::verify_systemd_service,
            service_commands::reload_systemd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
