#![windows_subsystem = "windows"]

mod app;
mod bar;
mod config;
mod gfx;
mod feeds;
mod droptarget;
mod i18n;
mod integrations;
mod launcher;
mod devices;
mod hotkeys;
mod http;
mod picker;
mod screentime;
mod media;
mod menu;
mod monitors;
mod notch;
mod notify;
mod pet;
mod preview;
mod road;
mod settings;
mod skin;
mod system;
mod sprites;
mod sysinfo;
mod taskbar;
mod tray;
mod trayflyout;
mod util;
mod weather;
mod widgets;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    config::migrate_dir();
    i18n::set("auto");
    if args.iter().any(|a| a == "--hook" || a == "--notify") {
        if let Ok(c) = config::load() {
            i18n::set(&c.general.language);
        }
    }
    if args.iter().any(|a| a == "--hook") {
        notify::run_hook();
        return;
    }
    if let Some(n) = notify::from_args(&args) {
        std::process::exit(if notify::send(&n) { 0 } else { 1 });
    }
    if args.iter().any(|a| a == "--integrate") {
        integrations::heal();
        return;
    }
    if args.iter().any(|a| a == "--unregister") {
        integrations::unregister();
        return;
    }
    if args.iter().any(|a| a == "--restore") {
        taskbar::show_all();
        return;
    }
    std::panic::set_hook(Box::new(|info| {
        taskbar::restore();
        let _ = std::fs::write(config::dir().join("crash.log"), info.to_string());
    }));
    if let Err(e) = app::run() {
        taskbar::restore();
        util::message_box(&if i18n::en() { format!("Taskbaroque could not start: {e}") } else { format!("Taskbaroque başlatılamadı: {e}") });
    }
}
