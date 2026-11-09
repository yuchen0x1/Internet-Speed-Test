// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // CLI 模式:GUI 子系统默认无控制台,先附着/分配控制台再输出
    if args.iter().any(|a| a == "cli" || a == "--cli") {
        ensure_console();
        std::process::exit(netvision_pro_lib::cli::run_cli(&args));
    }
    netvision_pro_lib::run()
}

/// GUI 子系统下为 CLI 模式恢复控制台输出:
/// - 输出已被重定向(管道/文件)时不动标准句柄,保证可被脚本捕获;
/// - 交互调用时附着父进程(终端)控制台,失败则分配新控制台。
#[cfg(windows)]
fn ensure_console() {
    unsafe {
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::System::Console::{
            AllocConsole, AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE,
        };
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        let redirected = !h.is_null() && h != INVALID_HANDLE_VALUE;
        if redirected {
            return;
        }
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            let _ = AllocConsole();
        }
    }
}

#[cfg(not(windows))]
fn ensure_console() {}
