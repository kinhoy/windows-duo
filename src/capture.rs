use anyhow::{anyhow, Result};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
    GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HORZRES, SRCCOPY, VERTRES,
};

pub struct ScreenCapture {
    pub width: u32,
    pub height: u32,
    /// BGRA，自上而下。
    pub pixels: Vec<u8>,
}

pub fn capture_primary_screen() -> Result<ScreenCapture> {
    unsafe {
        let screen_dc = GetDC(HWND(0));
        if screen_dc.is_invalid() {
            return Err(anyhow!("GetDC failed"));
        }
        let width = windows::Win32::Graphics::Gdi::GetDeviceCaps(screen_dc, HORZRES) as u32;
        let height = windows::Win32::Graphics::Gdi::GetDeviceCaps(screen_dc, VERTRES) as u32;
        if width == 0 || height == 0 {
            ReleaseDC(HWND(0), screen_dc);
            return Err(anyhow!("empty screen"));
        }

        let mem_dc = CreateCompatibleDC(screen_dc);
        let bitmap = CreateCompatibleBitmap(screen_dc, width as i32, height as i32);
        let old = SelectObject(mem_dc, bitmap);

        if BitBlt(mem_dc, 0, 0, width as i32, height as i32, screen_dc, 0, 0, SRCCOPY).is_err() {
            SelectObject(mem_dc, old);
            let _ = DeleteObject(bitmap);
            let _ = DeleteDC(mem_dc);
            ReleaseDC(HWND(0), screen_dc);
            return Err(anyhow!("BitBlt failed"));
        }

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        GetDIBits(
            mem_dc,
            bitmap,
            0,
            height,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut info,
            DIB_RGB_COLORS,
        );

        SelectObject(mem_dc, old);
        let _ = DeleteObject(bitmap);
        let _ = DeleteDC(mem_dc);
        ReleaseDC(HWND(0), screen_dc);

        Ok(ScreenCapture { width, height, pixels })
    }
}