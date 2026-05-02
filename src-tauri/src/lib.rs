mod commands;

use commands::{download, java, launch, mods, settings, workspace};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
            mods::remove_workspace_mod,
            mods::sync_workspace_mods,
            download::download_mc_version,
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
