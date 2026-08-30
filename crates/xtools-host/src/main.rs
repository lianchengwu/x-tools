mod ai_config;
mod ai_highlight;
mod ai_runtime;
mod anim;
mod layout;
mod runner;
mod settings_window;
mod window_prefs;

#[cfg(unix)]
mod paint;

#[cfg(unix)]
mod input;
#[cfg(unix)]
mod overlay;
#[cfg(unix)]
mod tray;
#[cfg(unix)]
mod unix;

fn print_help() {
    println!("xtools v0.4.0 (Unified WASM Floating Toolbox)");
    println!("Usage:");
    println!("  xtools                    # Start floating orb and system tray (Host mode)");
    println!("  xtools host               # Start floating orb and system tray");
    println!("  xtools run <plugin>       # Launch specific WASM plugin window");
    println!("  xtools <plugin.wasm>      # Directly launch WASM plugin by path or name");
    println!("  xtools settings           # Open the settings window (Baidu / AI config)");
    println!("  xtools list               # List all discovered WASM plugins");
    println!("  xtools --help             # Show this help information");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let first_arg = args.get(1).map(|s| s.as_str());

    match first_arg {
        None | Some("host") => {
            env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

            #[cfg(unix)]
            unix::run();

            #[cfg(not(unix))]
            {
                eprintln!("Windows host follows the same engine.");
            }
        }
        Some("-h") | Some("--help") | Some("help") => {
            print_help();
        }
        Some("settings") => {
            if let Err(e) = settings_window::run_settings() {
                eprintln!("xtools error: {e}");
                std::process::exit(1);
            }
        }
        Some("list") => {
            runner::list_plugins();
        }
        Some("run") => {
            let plugin = args.get(2).map(|s| s.as_str()).unwrap_or("time");
            if let Err(e) = runner::run_plugin(plugin) {
                eprintln!("xtools error: {e}");
                std::process::exit(1);
            }
        }
        Some(other)
            if other.ends_with(".wasm")
                || other.starts_with("xtools.")
                || other == "time"
                || other == "json"
                || other == "trans"
                || other == "ai" =>
        {
            if let Err(e) = runner::run_plugin(other) {
                eprintln!("xtools error: {e}");
                std::process::exit(1);
            }
        }
        Some(other) => {
            eprintln!("Unknown command: {other}");
            print_help();
            std::process::exit(1);
        }
    }
}
