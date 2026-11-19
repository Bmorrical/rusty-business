mod attachments;
mod downloads;
mod engine;
mod gate;
mod makers;
mod models;
mod outside;
mod projects;
mod sessions;
mod stack;
mod starter;
mod update;
mod workspace;

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
            // The app's own models folder, which `llama-server` reads on
            // Windows and Linux. A Mac's engine reads the Hugging Face cache.
            models::use_own_folder(data.join("models"));
            app.manage(sessions::Store::new(data.join("sessions")));
            app.manage(projects::Projects::new(data.join("projects"), data.join("memory.md")));
            app.manage(starter::Starter::new(data.join("engine.log")));
            // The outbound gate: off until the person turns it on.
            let gate = gate::Gate::new(data.join("gate"));
            app.manage(stack::Stack::new(data.join("stack"), data.join("stack.log"), stack::FRONT_DOOR_PORT, gate.clone()));
            app.manage(gate);
            // What a download left when the app last quit part way is thrown away.
            let downloads = downloads::Downloads::new(data.join("downloads"));
            downloads.clear();
            app.manage(downloads);
            // The model makers picked: Microsoft until the person picks others.
            app.manage(makers::Makers::new(data.join("makers"), &models::on_disk()));
            // Engines outside this computer: the path to each key, never the key.
            app.manage(outside::Outside::new(data.join("outside.json")));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine::engine_status,
            engine::chat_stream,
            engine::stop_chat,
            starter::start_engine,
            starter::free_memory,
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
            workspace::workspace_overview,
            workspace::workspace_tool,
            workspace::pick_folder,
            attachments::read_attachment,
            attachments::pick_files,
            update::check_update,
            update::open_update,
            gate::gate_status,
            gate::set_gate,
            makers::makers_status,
            makers::set_makers,
            outside::outside_status,
            outside::add_outside,
            outside::refresh_outside,
            outside::remove_outside,
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
