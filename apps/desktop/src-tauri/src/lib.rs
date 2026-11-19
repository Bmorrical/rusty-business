mod downloads;
mod engine;
mod models;
mod projects;
mod sessions;
mod stack;
mod starter;
mod update;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The name the app gives itself, as in its About window and the Mac's app
    // menu, carries its mark. Its file name and the folder its data is kept in
    // do not change.
    let mut context = tauri::generate_context!();
    context.package_info_mut().name = "Rusty Business™".to_string();

    let app = tauri::Builder::default()
        .manage(engine::Chats::default())
        .manage(update::Updates::default())
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            app.manage(sessions::Store::new(data.join("sessions")));
            app.manage(projects::Projects::new(data.join("projects"), data.join("memory.md")));
            app.manage(starter::Starter::new(data.join("engine.log")));
            app.manage(stack::Stack::new(data.join("stack"), data.join("stack.log"), stack::FRONT_DOOR_PORT));
            // What a download left when the app last quit part way is thrown away.
            let downloads = downloads::Downloads::new(data.join("downloads"));
            downloads.clear();
            app.manage(downloads);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine::engine_status,
            engine::chat_stream,
            engine::stop_chat,
            starter::start_engine,
            stack::start_stack,
            downloads::download_model,
            downloads::stop_download,
            sessions::list_sessions,
            sessions::load_session,
            sessions::save_session,
            sessions::trash_session,
            sessions::search_sessions,
            sessions::arrange_session,
            projects::list_projects,
            projects::save_project,
            projects::trash_project,
            projects::load_memory,
            projects::save_memory,
            projects::remember,
            update::check_update,
            update::open_update,
        ])
        .build(context)
        .expect("error while building tauri application");

    // When the app quits, it stops the engine and the stack it started. One
    // that was already running is not the app's to stop.
    app.run(|app, event| {
        if let tauri::RunEvent::Exit = event {
            if let Some(starter) = app.try_state::<starter::Starter>() {
                starter.stop();
            }
            if let Some(stack) = app.try_state::<stack::Stack>() {
                stack.stop();
            }
        }
    });
}
