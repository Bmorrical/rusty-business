mod engine;
mod sessions;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(engine::Chats::default())
        .setup(|app| {
            let folder = app.path().app_data_dir()?.join("sessions");
            app.manage(sessions::Store::new(folder));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine::engine_status,
            engine::chat_stream,
            engine::stop_chat,
            sessions::list_sessions,
            sessions::load_session,
            sessions::save_session,
            sessions::trash_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
