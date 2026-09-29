#[cfg(test)]
pub mod env_lock;

pub mod commands;
pub mod desktop_entry;
pub mod error;
pub mod filesystem;
pub mod icons;
pub mod locations;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::create_launcher,
            commands::open_desktop_entry,
            commands::save_desktop_entry,
            commands::delete_launcher,
            commands::list_managed_launchers,
            commands::list_desktop_locations,
            commands::validate_form,
            commands::run_desktop_file_validate,
            commands::find_nearby_icons,
            commands::stat_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
