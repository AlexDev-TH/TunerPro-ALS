//! The link to TunerPro when its plug-in started this program: which files TunerPro has
//! open, and how to hand a finished definition back to it.
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct Link {
    /// TunerPro's main window, as the plug-in passed it.
    pub window: Option<isize>,
    /// The bin TunerPro has open.
    pub bin: Option<PathBuf>,
    /// The definition TunerPro has open, if any.
    pub xdf: Option<PathBuf>,
    /// Whether TunerPro was showing its dark theme when this program started, if that could
    /// be seen.
    pub dark: Option<bool>,
}

/// Marks the message as ours, so the plug-in ignores anything else: "ADXF".
const LOAD_XDF: usize = 0x4144_5846;

#[cfg(windows)]
mod sys {
    use core::ffi::c_void;
    #[repr(C)]
    pub struct CopyData {
        pub kind: usize,
        pub len: u32,
        pub data: *const c_void,
    }
    #[repr(C)]
    pub struct IconInfo {
        pub icon: i32,
        pub x: u32,
        pub y: u32,
        pub mask: isize,
        pub color: isize,
    }
    #[repr(C)]
    pub struct Bitmap {
        pub kind: i32,
        pub width: i32,
        pub height: i32,
        pub width_bytes: i32,
        pub planes: u16,
        pub bits_pixel: u16,
        pub bits: *mut c_void,
    }
    #[repr(C)]
    #[derive(Default)]
    pub struct BitmapInfo {
        pub size: u32,
        pub width: i32,
        pub height: i32,
        pub planes: u16,
        pub bit_count: u16,
        pub compression: u32,
        pub size_image: u32,
        pub x_per_meter: i32,
        pub y_per_meter: i32,
        pub colors_used: u32,
        pub colors_important: u32,
        pub colors: [u32; 3],
    }
    #[repr(C)]
    #[derive(Default)]
    pub struct RectL {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }
    pub const WM_COPYDATA: u32 = 0x004A;
    pub const SMTO_ABORTIFHUNG: u32 = 0x0002;
    pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    /// Only the inside of the window, drawn the way the screen shows it.
    pub const PW_CLIENT_AS_SHOWN: u32 = 0x1 | 0x2;
    #[link(name = "user32")]
    extern "system" {
        pub fn IsIconic(hwnd: isize) -> i32;
        pub fn GetClientRect(hwnd: isize, rect: *mut RectL) -> i32;
        pub fn PrintWindow(hwnd: isize, dc: isize, flags: u32) -> i32;
        pub fn IsWindow(hwnd: isize) -> i32;
        pub fn SendMessageTimeoutW(hwnd: isize, msg: u32, wparam: usize, lparam: isize, flags: u32, timeout: u32, result: *mut usize) -> isize;
        pub fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
        pub fn GetIconInfo(icon: isize, info: *mut IconInfo) -> i32;
        pub fn DestroyIcon(icon: isize) -> i32;
        pub fn GetDC(hwnd: isize) -> isize;
        pub fn ReleaseDC(hwnd: isize, dc: isize) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        pub fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        pub fn QueryFullProcessImageNameW(process: isize, flags: u32, name: *mut u16, size: *mut u32) -> i32;
        pub fn CloseHandle(handle: isize) -> i32;
    }
    #[link(name = "shell32")]
    extern "system" {
        pub fn ExtractIconExW(file: *const u16, index: i32, large: *mut isize, small: *mut isize, count: u32) -> u32;
    }
    #[link(name = "gdi32")]
    extern "system" {
        pub fn GetObjectW(handle: isize, size: i32, out: *mut c_void) -> i32;
        pub fn GetDIBits(dc: isize, bitmap: isize, start: u32, lines: u32, bits: *mut c_void, info: *mut BitmapInfo, usage: u32) -> i32;
        pub fn DeleteObject(handle: isize) -> i32;
        pub fn CreateCompatibleDC(dc: isize) -> isize;
        pub fn CreateCompatibleBitmap(dc: isize, width: i32, height: i32) -> isize;
        pub fn SelectObject(dc: isize, object: isize) -> isize;
        pub fn DeleteDC(dc: isize) -> i32;
    }
}

/// Whether a picture is mostly dark. `None` for a picture that is black all over, which is
/// what a window that wouldn't draw itself gives.
fn mostly_dark(bgra: &[u8]) -> Option<bool> {
    let (mut dark, mut black, mut n) = (0usize, 0usize, 0usize);
    for p in bgra.chunks_exact(4) {
        let sum = p[0] as u32 + p[1] as u32 + p[2] as u32;
        n += 1;
        if sum < 330 {
            dark += 1;
        }
        if sum == 0 {
            black += 1;
        }
    }
    if n == 0 || black * 100 > n * 98 {
        return None;
    }
    Some(dark * 2 > n)
}

/// Whether TunerPro is showing its dark theme right now, judged from a picture of the top of
/// its window, where the toolbars and panel heads are. Looking at the window itself works
/// for every TunerPro version, whatever it calls the setting.
#[cfg(windows)]
pub fn host_is_dark(link: &Link) -> Option<bool> {
    let window = link.window?;
    unsafe {
        if sys::IsWindow(window) == 0 || sys::IsIconic(window) != 0 {
            return None;
        }
        let mut rc = sys::RectL::default();
        if sys::GetClientRect(window, &mut rc) == 0 {
            return None;
        }
        let (w, h) = ((rc.right - rc.left).min(2000), (rc.bottom - rc.top).min(300));
        if w < 60 || h < 20 {
            return None;
        }
        let screen = sys::GetDC(0);
        let mem = sys::CreateCompatibleDC(screen);
        let bitmap = sys::CreateCompatibleBitmap(screen, w, h);
        let before = sys::SelectObject(mem, bitmap);
        let drawn = sys::PrintWindow(window, mem, sys::PW_CLIENT_AS_SHOWN);
        sys::SelectObject(mem, before);
        let mut px = vec![0u8; (w * h * 4) as usize];
        let mut want = sys::BitmapInfo { size: 40, width: w, height: -h, planes: 1, bit_count: 32, ..Default::default() };
        let rows = sys::GetDIBits(screen, bitmap, 0, h as u32, px.as_mut_ptr().cast(), &mut want, 0);
        sys::DeleteObject(bitmap);
        sys::DeleteDC(mem);
        sys::ReleaseDC(0, screen);
        if drawn == 0 || rows == 0 {
            return None;
        }
        mostly_dark(&px)
    }
}
#[cfg(not(windows))]
pub fn host_is_dark(_link: &Link) -> Option<bool> {
    None
}

/// A copy of TunerPro installed on this computer.
fn installed_tunerpro() -> Option<PathBuf> {
    let mut found = vec![];
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        let Some(base) = std::env::var_os(var) else { continue };
        let Ok(dir) = std::fs::read_dir(base) else { continue };
        for entry in dir.flatten() {
            let exe = entry.path().join("TunerPro.exe");
            if entry.file_name().to_string_lossy().starts_with("TunerPro") && exe.is_file() {
                found.push(exe);
            }
        }
    }
    found.sort();
    found.into_iter().next()
}

/// One picture of an icon as RGBA, top row first.
#[cfg(windows)]
unsafe fn icon_pixels(info: &sys::IconInfo) -> Option<egui::IconData> {
    if info.color == 0 {
        return None;
    }
    let mut bm: sys::Bitmap = unsafe { std::mem::zeroed() };
    if unsafe { sys::GetObjectW(info.color, std::mem::size_of::<sys::Bitmap>() as i32, (&mut bm as *mut sys::Bitmap).cast()) } == 0 {
        return None;
    }
    let (w, h) = (bm.width as u32, bm.height as u32);
    if w == 0 || h == 0 || w > 512 || h > 512 {
        return None;
    }
    let read = |bitmap: isize| -> Option<Vec<u8>> {
        let mut px = vec![0u8; (w * h * 4) as usize];
        // A negative height asks for the top row first; 32 bits a pixel.
        let mut want = sys::BitmapInfo { size: 40, width: w as i32, height: -(h as i32), planes: 1, bit_count: 32, ..Default::default() };
        let rows = unsafe {
            let dc = sys::GetDC(0);
            let rows = sys::GetDIBits(dc, bitmap, 0, h, px.as_mut_ptr().cast(), &mut want, 0);
            sys::ReleaseDC(0, dc);
            rows
        };
        (rows != 0).then_some(px)
    };
    let mut px = read(info.color)?;
    let mask = if info.mask != 0 { read(info.mask) } else { None };
    let has_alpha = px.chunks_exact(4).any(|p| p[3] != 0);
    for (i, p) in px.chunks_exact_mut(4).enumerate() {
        p.swap(0, 2); // stored blue first
        if !has_alpha {
            // Older icons say what shows through in a second picture instead: white is clear.
            p[3] = if mask.as_ref().is_some_and(|m| m[i * 4] != 0) { 0 } else { 255 };
        }
    }
    Some(egui::IconData { rgba: px, width: w, height: h })
}

/// The picture for the title bar: TunerPro's own icon, read from the copy of TunerPro on this
/// computer, so the window looks like the part of TunerPro it is. None of TunerPro's files
/// ship with this program; where TunerPro isn't found the window keeps its default picture.
#[cfg(windows)]
pub fn host_icon(link: &Link) -> Option<egui::IconData> {
    use std::os::windows::ffi::OsStrExt;
    let exe = link.host_exe()?;
    let wide: Vec<u16> = exe.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    unsafe {
        let mut icon = 0isize;
        if sys::ExtractIconExW(wide.as_ptr(), 0, &mut icon, std::ptr::null_mut(), 1) == 0 || icon == 0 {
            return None;
        }
        let mut info: sys::IconInfo = std::mem::zeroed();
        let pixels = if sys::GetIconInfo(icon, &mut info) != 0 { icon_pixels(&info) } else { None };
        for handle in [info.mask, info.color] {
            if handle != 0 {
                sys::DeleteObject(handle);
            }
        }
        sys::DestroyIcon(icon);
        pixels
    }
}
#[cfg(not(windows))]
pub fn host_icon(_link: &Link) -> Option<egui::IconData> {
    None
}

impl Link {
    pub fn connected(&self) -> bool {
        self.window.is_some()
    }

    /// TunerPro's program file: the one behind its window, or else an installed copy.
    #[cfg(windows)]
    fn host_exe(&self) -> Option<PathBuf> {
        use std::os::windows::ffi::OsStringExt;
        let running = self.window.and_then(|w| unsafe {
            let mut pid = 0u32;
            sys::GetWindowThreadProcessId(w, &mut pid);
            let process = if pid == 0 { 0 } else { sys::OpenProcess(sys::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
            if process == 0 {
                return None;
            }
            let mut name = [0u16; 1024];
            let mut len = name.len() as u32;
            let ok = sys::QueryFullProcessImageNameW(process, 0, name.as_mut_ptr(), &mut len);
            sys::CloseHandle(process);
            (ok != 0).then(|| PathBuf::from(std::ffi::OsString::from_wide(&name[..len as usize])))
        });
        running.or_else(installed_tunerpro)
    }

    /// Asks TunerPro to open this definition. The plug-in answers once it has taken the path;
    /// TunerPro then loads the file on its own time and may ask about unsaved changes.
    #[cfg(windows)]
    pub fn load_xdf(&self, path: &Path) -> Result<(), String> {
        use std::os::windows::ffi::OsStrExt;
        let window = self.window.filter(|w| unsafe { sys::IsWindow(*w) } != 0).ok_or("TunerPro is no longer open.")?;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let msg = sys::CopyData { kind: LOAD_XDF, len: (wide.len() * 2) as u32, data: wide.as_ptr().cast() };
        let mut answer = 0usize;
        let sent = unsafe { sys::SendMessageTimeoutW(window, sys::WM_COPYDATA, 0, &msg as *const sys::CopyData as isize, sys::SMTO_ABORTIFHUNG, 5000, &mut answer) };
        if sent == 0 {
            return Err("TunerPro didn't answer.".into());
        }
        if answer == 0 {
            return Err("TunerPro didn't take the file. Is the ALS plug-in still loaded?".into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    pub fn load_xdf(&self, _path: &Path) -> Result<(), String> {
        Err("TunerPro only runs on Windows.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_judged_dark_or_light_by_most_of_it() {
        let px = |v: u8, n: usize| vec![v; n * 4];
        assert_eq!(mostly_dark(&px(0x2B, 100)), Some(true), "TunerPro's dark bars");
        assert_eq!(mostly_dark(&px(0xF0, 100)), Some(false), "a light window");
        let mut mixed = px(0x1E, 70);
        mixed.extend(px(0xFF, 30));
        assert_eq!(mostly_dark(&mixed), Some(true), "dark with light symbols on it");
        assert_eq!(mostly_dark(&px(0, 100)), None, "black all over is a window that didn't draw");
        assert_eq!(mostly_dark(&[]), None);
    }
}
