// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if on_air_desktop_lib::exit_if_hidden_dev_launch() {
        return;
    }
    on_air_desktop_lib::run()
}
