//! 监听 Windows 电源事件 + 托盘图标。
//!
//! 用一个隐藏窗口接收 `WM_POWERBROADCAST`（盖子开合）和自定义的
//! `WM_TRAYICON`（托盘图标交互）。事件通过 mpsc 转发到主线程。

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::OnceLock;
use std::thread;

use windows::core::{GUID, PCWSTR};
use windows::Win32::Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::RegisterPowerSettingNotification;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowW, GetMessageW, LoadIconW,
    PostMessageW, RegisterClassW, TranslateMessage, DEVICE_NOTIFY_WINDOW_HANDLE, HMENU, MSG,
    WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW, CS_HREDRAW, CS_VREDRAW, IDI_APPLICATION, WM_USER,
};

const WM_POWERBROADCAST: u32 = 0x0218;
const PBT_POWERSETTINGCHANGE: u32 = 0x8013;

/// 托盘图标发回窗口的自定义消息。
const WM_TRAYICON: u32 = WM_USER + 1;
/// 双击消息。
const WM_LBUTTONDBLCLK: u32 = 0x0203;

/// 隐藏窗口的类名，第二个实例用这个找第一个实例。
const WATCHER_CLASS: &str = "WindowsDuoPowerWatcher";

// {BA3E0F4D-B817-4094-A2D1-D56379E6A0F3}
const GUID_LIDSWITCH_STATE_CHANGE: GUID =
    GUID::from_u128(0xBA3E0F4D_B817_4094_A2D1_D56379E6A0F3);

#[derive(Debug, Clone, Copy)]
pub enum PowerEvent {
    LidClosed,
    LidOpened,
    /// 用户双击了托盘图标，或第二个实例请求唤出窗口。
    TrayDoubleClick,
}

static SENDER: OnceLock<Sender<PowerEvent>> = OnceLock::new();

pub struct PowerWatcher {
    receiver: Receiver<PowerEvent>,
}

impl PowerWatcher {
    pub fn start() -> Self {
        let (tx, rx) = channel();
        let _ = SENDER.set(tx);

        let _ = thread::Builder::new()
            .name("power-watcher".into())
            .spawn(|| unsafe {
                if let Err(e) = run_message_loop() {
                    log::warn!("power watcher stopped: {e:?}");
                }
            });

        Self { receiver: rx }
    }

    pub fn try_recv(&self) -> Option<PowerEvent> {
        self.receiver.try_recv().ok()
    }
}

/// 第二个实例调用：找到第一个实例的隐藏窗口，让它弹出设置界面。
/// 找不到（第一个实例还没起窗口）就什么都不做。
pub fn signal_existing_instance() {
    unsafe {
        let class: Vec<u16> = format!("{WATCHER_CLASS}\0").encode_utf16().collect();
        // windows-rs 0.52 里 FindWindowW 直接返回 HWND，找不到时 .0 == 0。
        let hwnd = FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null());
        if hwnd.0 != 0 {
            let _ = PostMessageW(
                hwnd,
                WM_TRAYICON,
                WPARAM(1),
                LPARAM(WM_LBUTTONDBLCLK as isize),
            );
        }
    }
}

unsafe fn run_message_loop() -> windows::core::Result<()> {
    let module = GetModuleHandleW(PCWSTR::null())?;
    let instance = HINSTANCE(module.0);

    let class_name: Vec<u16> = format!("{WATCHER_CLASS}\0").encode_utf16().collect();
    let class_name = PCWSTR(class_name.as_ptr());

    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: Default::default(),
        hCursor: Default::default(),
        hbrBackground: Default::default(),
        lpszMenuName: PCWSTR::null(),
        lpszClassName: class_name,
    };
    RegisterClassW(&wc);

    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class_name,
        PCWSTR::null(),
        WINDOW_STYLE::default(),
        0,
        0,
        0,
        0,
        HWND::default(),
        HMENU::default(),
        instance,
        None,
    );

    if hwnd.0 == 0 {
        return Err(windows::core::Error::from_win32());
    }

    // 电源通知（盖子）
    let _notify = RegisterPowerSettingNotification(
        HANDLE(hwnd.0),
        &GUID_LIDSWITCH_STATE_CHANGE,
        DEVICE_NOTIFY_WINDOW_HANDLE.0,
    );

    // 托盘图标
    let hicon = LoadIconW(HINSTANCE::default(), IDI_APPLICATION).unwrap_or_default();

    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAYICON;
    nid.hIcon = hicon;
    let tip: Vec<u16> = "Windows Duo — 双击打开设置\0".encode_utf16().collect();
    for (i, &c) in tip.iter().take(128).enumerate() {
        nid.szTip[i] = c;
    }

    let added = Shell_NotifyIconW(NIM_ADD, &nid).as_bool();
    if added {
        log::info!("tray icon added");
    } else {
        log::warn!("Shell_NotifyIconW(NIM_ADD) failed");
    }

    log::info!("power watcher listening for lid switch");

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    Ok(())
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_POWERBROADCAST && wparam.0 as u32 == PBT_POWERSETTINGCHANGE {
        #[repr(C)]
        struct Setting {
            power_setting: GUID,
            data_length: u32,
            data: [u8; 1],
        }
        let setting = &*(lparam.0 as *const Setting);
        if setting.power_setting == GUID_LIDSWITCH_STATE_CHANGE {
            let state = *(setting.data.as_ptr() as *const u32);
            let event = if state != 0 {
                PowerEvent::LidOpened
            } else {
                PowerEvent::LidClosed
            };
            log::info!("lid switch event: {event:?}");
            if let Some(tx) = SENDER.get() {
                let _ = tx.send(event);
            }
        }
        return LRESULT(0);
    }

    if msg == WM_TRAYICON {
        let mouse = lparam.0 as u32;
        if mouse == WM_LBUTTONDBLCLK {
            log::info!("tray double-click (or second-instance signal)");
            if let Some(tx) = SENDER.get() {
                let _ = tx.send(PowerEvent::TrayDoubleClick);
            }
        }
        return LRESULT(0);
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}