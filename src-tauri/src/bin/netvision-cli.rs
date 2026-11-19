//! Console-native entry point; no WebView, tray or display server dependency.
fn main() {
    let mut args: Vec<String> = std::env::args().collect();
    if matches!(args.get(1).map(String::as_str), Some("--version" | "-V")) {
        println!("NetVision Pro CLI {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if matches!(
        args.get(1).map(String::as_str),
        None | Some("--help" | "-h")
    ) {
        println!("NetVision Pro CLI\n\n  netvision-cli speedtest [--json] [--duration 5-30] [--no-save]\n  netvision-cli history list [--json]\n  netvision-cli history export [--format csv|json]\n\nNETVISION_DATA_DIR overrides the local data directory.\nProgress: stderr; results: stdout. Exit: 0 complete, 1 arguments, 2 cancelled, 3 failed.");
        return;
    }
    if !matches!(args.get(1).map(String::as_str), Some("cli" | "--cli")) {
        args.insert(1, "cli".into());
    }
    std::process::exit(netvision_pro_lib::cli::run_cli(&args));
}
