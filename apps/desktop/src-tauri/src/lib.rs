mod engine;
mod models;
mod sessions;
mod starter;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .manage(engine::Chats::default())
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            app.manage(sessions::Store::new(data.join("sessions")));
            app.manage(starter::Starter::new(data.join("engine.log")));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine::engine_status,
            engine::chat_stream,
            engine::stop_chat,
            starter::start_engine,
            sessions::list_sessions,
            sessions::load_session,
            sessions::save_session,
            sessions::trash_session,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // When the app quits, it stops the engine it started. An engine that was
    // already running is not the app's to stop.
    app.run(|app, event| {
        if let tauri::RunEvent::Exit = event
            && let Some(starter) = app.try_state::<starter::Starter>()
        {
            starter.stop();
        }
    });
}
