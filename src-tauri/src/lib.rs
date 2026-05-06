mod commands;

use commands::{export, java, launch, mods, settings, workspace};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|_app| {
            if let Err(err) = java::auto_register_system_java() {
                eprintln!("[mmpc] auto register system java failed: {err}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            workspace::list_workspaces,
            workspace::list_release_versions,
            workspace::list_fabric_loader_versions,
            workspace::list_forge_loader_versions,
            workspace::list_neoforge_loader_versions,
            workspace::create_workspace,
            workspace::delete_workspace,
            workspace::save_pack_config,
            workspace::get_pack_config,
            mods::search_modrinth_mods,
            mods::install_modrinth_mod,
            mods::list_workspace_mods,
            mods::remove_workspace_mod,
            mods::set_workspace_mod_enabled,
            mods::update_workspace_mod_type,
            export::export_workspace,
            launch::launch_game,
            launch::stop_game,
            java::list_java_runtimes,
            java::detect_java_runtime,
            java::add_java_runtime,
            java::delete_java_runtime,
            settings::get_settings,
            settings::save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
