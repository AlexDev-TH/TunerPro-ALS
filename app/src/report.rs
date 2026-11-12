//! Starting up on computers that differ from the one the program was built on: which way of
//! drawing to use, and saying what went wrong instead of leaving without a word.
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Set when the program runs unattended (a picture for checking), so no message waits for a click.
static QUIET: AtomicBool = AtomicBool::new(false);
/// Frames drawn since the start.
static FRAMES: AtomicU32 = AtomicU32::new(0);

/// How the window is drawn.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Drawing {
    /// Direct3D 12 or Vulkan, the usual way.
    Modern,
    /// OpenGL, for older graphics chips and drivers.
    OpenGl,
}

impl Drawing {
    pub fn name(self) -> &'static str {
        match self {
            Drawing::Modern => "Direct3D/Vulkan",
            Drawing::OpenGl => "OpenGL",
        }
    }
}

pub fn set_quiet(quiet: bool) {
    QUIET.store(quiet, Ordering::Relaxed);
}

/// Notes about starting, for finding out why a computer could not show the window.
pub fn log_path() -> PathBuf {
    crate::config::data_home().join("ALS-start.log")
}

/// Marks that the usual way of drawing is being tried. It is taken away once frames are on
/// the screen, so finding it at the next start means the program died in between.
fn guard_path() -> PathBuf {
    crate::config::data_home().join("starting")
}

/// Holds "opengl" once the usual way of drawing has brought the program down.
fn choice_path() -> PathBuf {
    crate::config::data_home().join("drawing.txt")
}

/// Adds a line to the start log. The log is started afresh when it has grown large.
pub fn note(line: &str) {
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 64 * 1024) {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "ALS {}: {line}", env!("CARGO_PKG_VERSION"));
    }
}

/// The ways of drawing to try, in order. `asked` is what the command line said, if anything.
pub fn drawing_order(asked: Option<Drawing>) -> Vec<Drawing> {
    if let Some(d) = asked {
        return vec![d];
    }
    if guard_path().exists() {
        // The last start never got as far as drawing: stay with OpenGL from now on.
        note("the last start ended while Direct3D/Vulkan was being set up; using OpenGL from now on");
        let _ = std::fs::write(choice_path(), "opengl");
        let _ = std::fs::remove_file(guard_path());
    }
    let opengl = std::fs::read_to_string(choice_path()).is_ok_and(|s| s.trim().eq_ignore_ascii_case("opengl"));
    if opengl {
        vec![Drawing::OpenGl]
    } else {
        vec![Drawing::Modern, Drawing::OpenGl]
    }
}

/// Called before a way of drawing is tried.
pub fn starting(drawing: Drawing) {
    FRAMES.store(0, Ordering::Relaxed);
    if drawing == Drawing::Modern {
        if let Some(dir) = guard_path().parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(guard_path(), "");
    }
}

/// Called when a way of drawing has ended, with or without an error.
pub fn ended() {
    let _ = std::fs::remove_file(guard_path());
}

/// Called once for every frame drawn.
pub fn frame_drawn() {
    if FRAMES.fetch_add(1, Ordering::Relaxed) == 2 {
        let _ = std::fs::remove_file(guard_path());
    }
}

/// Whether the window got as far as drawing.
pub fn drew_frames() -> bool {
    FRAMES.load(Ordering::Relaxed) > 0
}

/// Shows a message in a window of its own, unless the program runs unattended.
pub fn alert(text: &str) {
    if QUIET.load(Ordering::Relaxed) {
        eprintln!("{text}");
        return;
    }
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        extern "system" {
            fn MessageBoxW(window: isize, text: *const u16, caption: *const u16, kind: u32) -> i32;
        }
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (text, caption) = (wide(text), wide("ALS"));
        // On a thread of its own: the main thread may have a "quit" waiting in its queue from
        // the window that just failed, and a message box that meets it closes at once.
        // MB_OK | MB_ICONERROR | MB_SETFOREGROUND
        let _ = std::thread::spawn(move || unsafe { MessageBoxW(0, text.as_ptr(), caption.as_ptr(), 0x0001_0010) }).join();
    }
    #[cfg(not(windows))]
    eprintln!("{text}");
}

/// A fault inside the program is written to the log and shown, instead of the window just closing.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let text = info.to_string();
        note(&format!("stopped by a fault: {text}"));
        alert(&format!(
            "ALS หยุดทำงานเพราะข้อผิดพลาดในโปรแกรม\nALS stopped because of a fault in the program.\n\n{text}\n\nบันทึก / Log: {}",
            log_path().display()
        ));
    }));
}

/// Told when no way of drawing worked: what was tried and what each said.
pub fn no_window(tried: &[(Drawing, String)]) {
    let mut lines = String::new();
    for (d, e) in tried {
        lines.push_str(&format!("{}: {e}\n", d.name()));
    }
    alert(&format!(
        "ALS เปิดหน้าต่างไม่ได้ เพราะระบบกราฟิกของเครื่องนี้ใช้ไม่ได้ทั้ง Direct3D 12, Vulkan และ OpenGL\nลองอัปเดตไดรเวอร์การ์ดจอ แล้วเปิดใหม่\n\nALS could not open its window: Direct3D 12, Vulkan and OpenGL all failed on this computer.\nUpdating the graphics driver usually fixes this.\n\n{lines}\nบันทึก / Log: {}",
        log_path().display()
    ));
}
