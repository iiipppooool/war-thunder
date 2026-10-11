// Keep the macOS release build from opening a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    testdrive3_lib::run()
}
