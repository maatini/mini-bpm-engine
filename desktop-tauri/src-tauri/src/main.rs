#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod api_helpers;
mod commands;
mod sse_consumer;
mod state;


fn main() {
    let initial_state = state::AppState {
        client: reqwest::Client::new(),
        base_url: std::sync::Mutex::new(
            std::env::var("ENGINE_API_URL").unwrap_or_else(|_| "http://localhost:8081".to_string()),
        ),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(initial_state)
        .setup(|app| {
            use tauri::Manager;
            let app_handle = app.handle().clone();
            let state = app.state::<state::AppState>();
            let base_url = state.base_url.lock().map(|u: std::sync::MutexGuard<'_, String>| u.clone()).unwrap_or_else(|_| "http://localhost:8081".to_string());
            let client = state.client.clone();
            sse_consumer::spawn(app_handle, base_url, client);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::deploy::deploy_simple_process,
            commands::deploy::deploy_definition,
            commands::deploy::list_definitions,
            commands::deploy::get_definition_xml,
            commands::deploy::delete_definition,
            commands::deploy::delete_all_definitions,
            commands::instances::start_instance,
            commands::instances::start_timer_instance,
            commands::instances::list_instances,
            commands::instances::get_instance_details,
            commands::instances::get_instance_history,
            commands::instances::update_instance_variables,
            commands::instances::suspend_instance,
            commands::instances::resume_instance,
            commands::instances::move_token,
            commands::instances::delete_instance,
            commands::instances::query_completed_instances,
            commands::instances::get_completed_instance,
            commands::tasks::get_pending_tasks,
            commands::tasks::complete_task,
            commands::tasks::get_pending_service_tasks,
            commands::tasks::fetch_and_lock_service_tasks,
            commands::tasks::complete_service_task,
            commands::tasks::retry_incident,
            commands::tasks::resolve_incident,
            commands::tasks::get_pending_timers,
            commands::tasks::get_pending_message_catches,
            commands::files::upload_instance_file,
            commands::files::download_instance_file,
            commands::files::delete_instance_file,
            commands::monitoring::get_api_url,
            commands::monitoring::set_api_url,
            commands::monitoring::get_monitoring_data,
            commands::monitoring::get_bucket_entries,
            commands::monitoring::get_bucket_entry_detail,
            commands::monitoring::get_log_entries,
            commands::monitoring::read_bpmn_file,
            commands::messages::correlate_message
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
