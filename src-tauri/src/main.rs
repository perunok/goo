// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `goo --mcp-stdio`: the bridge AI assistants launch. It relays to the running app (starting it if needed) and
    // never opens a window itself, so it runs before Tauri (and its single-instance hand-over) starts.
    if std::env::args().skip(1).any(|a| a == goo_mcp::STDIO_FLAG) {
        let Some(dir) = goo_mcp::files::app_config_dir() else {
            eprintln!("goo: no config directory on this system");
            std::process::exit(1);
        };
        std::process::exit(goo_mcp::bridge::run(&dir));
    }
    goo_lib::run()
}
