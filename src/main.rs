#![windows_subsystem = "windows"]

mod app;
mod capture;
mod config;
mod depth;
mod power;
mod renderer;
mod simulation;

use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, BOOL, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

/// 单实例锁。返回 `Some` 时表示拿到锁，返回值必须持有到进程退出，
/// 否则系统会释放这个 mutex，别的实例就能再进来。
fn acquire_single_instance() -> Option<HANDLE> {
    unsafe {
        let handle =
            CreateMutexW(None, BOOL(0), w!("Global\\WindowsDuoInstance")).ok()?;
        // CreateMutexW 成功后如果 last error 是 183 (ERROR_ALREADY_EXISTS)，
        // 说明这个 mutex 已经存在，也就是已经有实例在跑。
        let already = std::io::Error::last_os_error().raw_os_error() == Some(183);
        if already {
            let _ = CloseHandle(handle);
            None
        } else {
            Some(handle)
        }
    }
}

fn main() -> anyhow::Result<()> {
    // 崩溃时写文件，方便排查（因为 windows_subsystem 隐藏了控制台）。
    std::panic::set_hook(Box::new(|info| {
        let dir = std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("windows-duo");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("crash.log"), format!("{info}\n"));
    }));

    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .init();

    // 单实例：已经有在跑就通知它弹窗口，自己立刻退出。
    let _instance_guard = match acquire_single_instance() {
        Some(handle) => handle,
        None => {
            power::signal_existing_instance();
            return Ok(());
        }
    };

    app::run()
}