// A release build on Windows is a window app: without this the system opens a console beside it.
// A debug build keeps the console, where the logs are printed.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    wyck::run();
}
