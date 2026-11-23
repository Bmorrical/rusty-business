// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `rusty-business board ...` does its work in the terminal and leaves.
    // Anything else opens the app.
    if let Some(code) = rusty_business_lib::command(std::env::args().skip(1).collect()) {
        std::process::exit(code);
    }
    rusty_business_lib::run()
}
