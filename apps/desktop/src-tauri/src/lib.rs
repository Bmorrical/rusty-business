mod attachments;
mod board;
mod cli;
mod courier;
mod deactivated;
mod downloads;
mod engine;
mod gate;
mod makers;
mod models;
mod outside;
mod picture;
mod projects;
mod remote;
mod rig;
mod rules;
mod sessions;
mod stack;
mod starter;
mod store;
mod sync;
mod uninstall;
mod update;
mod utilities;
mod vision;
mod web;
mod workspace;

use tauri::Manager;

/// The `board` command, for a terminal. It gives back what to leave with, or
/// `None` when the app was started the usual way and its window should open.
pub fn command(args: Vec<String>) -> Option<i32> {
    cli::run(&args)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The name the app gives itself, as in its About window and the Mac's app
    // menu, carries its mark. Its file name and the folder its data is kept in
    // do not change.
    let mut context = tauri::generate_context!();
    context.package_info_mut().name = "Rusty Business™".to_string();

    let app = tauri::Builder::default()
        // Checks a new version's signature and puts it in place. It reaches
        // nothing itself: `update::install_update` hands it what was fetched
        // through the outbound gate.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(engine::Chats::default())
        .manage(update::Updates::default())
        .manage(utilities::Made::default())
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            // The app's own models folder, which `llama-server` reads on
            // Windows and Linux. A Mac's engine reads the Hugging Face cache.
            models::use_own_folder(data.join("models"));
            // The app's resource folder, which holds the engine on Windows
            // and Linux. Tauri's own resource_dir reads the name we mark with
            // ™ below, so the folder is worked out from the un-marked name
            // and the place the binary is installed.
            let resource = app.config().product_name.as_deref().and_then(|name| {
                std::env::current_exe()
                    .ok()
                    .and_then(|exe| starter::resource_folder(&exe, name))
            });
            starter::use_resource_folder(resource);
            app.manage(sessions::Store::new(data.join("sessions")));
            app.manage(projects::Projects::new(
                data.join("projects"),
                data.join("memory.md"),
            ));
            app.manage(starter::Starter::new(data.join("engine.log")));
            // The picture engine, kept apart from the chat engine.
            app.manage(vision::PictureStarter(starter::Starter::new(
                data.join("picture-engine.log"),
            )));
            // Keep the existing WAN preference and start LAN off.
            let gate = gate::Gate::new(data.join("gate"));
            app.manage(gate::LanGate::new(data.join("lan-gate")));
            app.manage(stack::Stack::new(
                data.join("stack"),
                data.join("stack.log"),
                stack::FRONT_DOOR_PORT,
                gate.clone(),
            ));
            app.manage(gate);
            // What a download left when the app last quit part way is thrown away.
            let downloads = downloads::Downloads::new(data.join("downloads"));
            downloads.clear();
            app.manage(downloads);
            // The model makers picked: Microsoft until the person picks others.
            app.manage(makers::Makers::new(data.join("makers"), &models::on_disk()));
            // Engines outside this computer: the path to each key, never the key.
            app.manage(outside::Outside::new(data.join("outside.json")));
            // The models switched off: on this computer, or outside it.
            app.manage(deactivated::Deactivated::new(data.join("deactivated")));
            // Web search: the path to a Brave key, never the key.
            app.manage(web::Web::new(data.join("web.json")));
            // Boards: the local copies the Workflow view reads, and the notes on
            // them, in an encrypted database whose key is in the system's key store.
            app.manage(board::Boards::open(
                data.join("boards"),
                &store::SystemKeys::new(&app.config().identifier),
                |said| eprintln!("{said}"),
            ));
            // The picture check: its model, once the person installs it.
            app.manage(picture::PictureCheck::new(data.join("picture-check")));
            // The courier door: an MCP server on your private network, behind
            // its own circuit breaker (off until you turn it on) and the LAN gate.
            app.manage(courier::Courier::new(
                data.join("courier-breaker"),
                data.join("couriers.json"),
            ));
            courier::start(app.handle());
            // The rig plan: many models at once on servers on your private
            // network, behind the LAN gate.
            app.manage(rig::Rig::new(data.join("rig.json")));
            // The orchestrator's rules: coded checks on every move of a
            // slice, each behind its own toggle.
            app.manage(rules::Rules::new(data.join("rules.json")));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            web::web_status,
            web::set_web_key,
            web::remove_web_key,
            web::web_search,
            engine::engine_status,
            engine::chat_stream,
            engine::stop_chat,
            starter::start_engine,
            starter::free_memory,
            vision::picture_engine_status,
            vision::start_picture_engine,
            vision::read_picture,
            stack::start_stack,
            downloads::download_model,
            downloads::stop_download,
            board::list_boards,
            board::load_board,
            board::open_sample_board,
            board::save_board_note,
            rules::rules_status,
            rules::set_rule,
            rules::set_rule_limit,
            rules::check_move,
            rules::move_slice,
            sessions::list_sessions,
            sessions::list_votes,
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
            update::install_update,
            gate::gate_status,
            gate::set_gate,
            gate::lan_gate_status,
            gate::set_lan_gate,
            courier::courier_status,
            rig::rig_status,
            rig::save_rig,
            courier::set_courier,
            courier::pair_courier,
            courier::stop_pairing_courier,
            courier::unpair_courier,
            makers::makers_status,
            makers::set_makers,
            outside::outside_status,
            outside::add_outside,
            outside::refresh_outside,
            outside::remove_outside,
            deactivated::deactivated_models,
            deactivated::set_model_active,
            picture::picture_check_status,
            picture::install_picture_check,
            picture::uninstall_picture_check,
            picture::score_picture,
            picture::pick_picture,
            uninstall::uninstall_model,
            uninstall::uninstall_place,
            utilities::utilities_status,
            utilities::heic_pictures,
            utilities::heic_to_jpeg,
            utilities::pick_heic,
            utilities::show_jpeg,
        ])
        .build(context)
        .expect("error while building tauri application");

    app.run(|app, event| {
        if let tauri::RunEvent::Exit = event {
            stop_everything(app);
        }
    });
}

/// When the app quits, or a new version is put in its place, it stops the
/// engine and the stack it started. One that was already running is not the
/// app's to stop.
pub(crate) fn stop_everything(app: &tauri::AppHandle) {
    if let Some(starter) = app.try_state::<starter::Starter>() {
        starter.stop();
    }
    if let Some(stack) = app.try_state::<stack::Stack>() {
        stack.stop();
    }
}
