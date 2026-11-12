//! Drives the real screens with keys and mouse, without a window, the way a person would.
use crate::app::App;
use crate::cmd::Cmd;
use crate::dialogs::Modal;
use crate::model::*;
use crate::view::SelMode;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

struct Driver {
    ctx: egui::Context,
    app: App,
    time: f64,
}

impl Driver {
    fn new() -> Driver {
        let mut app = App::new(Some(std::path::PathBuf::from("unused.png")), None);
        app.screenshot = None;
        let mut d = Driver { ctx: egui::Context::default(), app, time: 0.0 };
        d.frames(3);
        d
    }

    fn frame(&mut self, events: Vec<Event>, mods: Modifiers) {
        self.time += 0.05;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1241.0, 779.0))),
            time: Some(self.time),
            events: std::iter::once(Event::ModifiersChanged(mods)).chain(events).collect(),
            ..RawInput::default()
        };
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(raw, |ui| app.frame_ui(ui));
        out.textures_delta.clear();
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame(vec![], Modifiers::NONE);
        }
    }

    fn typed(&mut self, t: &str) {
        for ch in t.chars() {
            self.frame(vec![Event::Text(ch.to_string())], Modifiers::NONE);
            self.frames(2);
        }
    }

    fn key(&mut self, key: Key, mods: Modifiers) {
        let down = Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: mods };
        let up = Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: mods };
        self.frame(vec![down], mods);
        self.frame(vec![up], mods);
        self.frames(2);
    }

    fn press_at(&mut self, pos: Pos2, down: bool, button: PointerButton, mods: Modifiers) {
        self.frame(vec![Event::PointerMoved(pos), Event::PointerButton { pos, button, pressed: down, modifiers: mods }], mods);
    }

    fn click(&mut self, pos: Pos2) {
        self.frame(vec![Event::PointerMoved(pos)], Modifiers::NONE);
        self.press_at(pos, true, PointerButton::Primary, Modifiers::NONE);
        self.press_at(pos, false, PointerButton::Primary, Modifiers::NONE);
        self.frames(3);
    }

    fn drag(&mut self, a: Pos2, b: Pos2) {
        self.frame(vec![Event::PointerMoved(a)], Modifiers::NONE);
        self.press_at(a, true, PointerButton::Primary, Modifiers::NONE);
        for k in 1..=6 {
            let p = a + (b - a) * (k as f32 / 6.0);
            self.frame(vec![Event::PointerMoved(p)], Modifiers::NONE);
        }
        self.press_at(b, false, PointerButton::Primary, Modifiers::NONE);
        self.frames(3);
    }

    fn sample(&mut self) {
        self.app.pending.push(Cmd::SampleProject);
        self.frames(3);
        // The search runs on its own thread; wait for it like the status bar does.
        let start = std::time::Instant::now();
        while !self.app.searches.is_empty() && start.elapsed().as_secs() < 30 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            self.frames(1);
        }
        self.frames(3);
    }

    fn cmd(&mut self, c: Cmd) {
        self.app.pending.push(c);
        self.frames(3);
    }

    fn g(&self) -> crate::view::Grid {
        self.app.active_child().expect("a window is open").g.clone()
    }

    fn p(&self) -> &Project {
        self.app.current_project().expect("a project is open")
    }

    /// Screen position of a cell in the active hexdump's text view.
    fn cell_pos(&self, i: usize) -> Pos2 {
        let c = self.app.active_child().expect("window");
        let g = &c.g;
        // Window frame, title, inner bar, then the same layout the view uses.
        let top = c.rect.top() + 1.0 + 24.0 + 25.0;
        let font = egui::FontId::monospace(g.font);
        let cw = self.ctx.fonts_mut(|f| f.glyph_width(&font, '0'));
        let lh = (g.font * 1.24_f32).round();
        let pitch = (g.cell_chars() as f32 + 1.0) * cw;
        let end = self.app.end_of(c);
        let digits = format!("{:X}", end.saturating_sub(1).max(0xFFFF)).len().max(5);
        let left = c.rect.left() + 1.0 + (digits as f32 + 1.0) * cw + 6.0;
        let row = (i / g.cols) as f32 - g.top_row as f32;
        Pos2::new(left + (i % g.cols) as f32 * pitch + pitch / 2.0, top + row * lh + lh / 2.0)
    }
}

const SHIFT: Modifiers = Modifiers { alt: false, ctrl: false, shift: true, mac_cmd: false, command: false };
const CTRL: Modifiers = Modifiers { alt: false, ctrl: true, shift: false, mac_cmd: false, command: true };

#[test]
fn start_is_empty_and_calm() {
    let mut d = Driver::new();
    assert!(d.app.projects.is_empty());
    assert!(d.app.modal.is_none());
    // Keys with nothing open do nothing bad.
    d.typed("fkm+-=%");
    d.key(Key::F3, Modifiers::NONE);
    assert!(d.app.modal.is_none());
}

#[test]
fn f_finds_maps_and_k_makes_one() {
    let mut d = Driver::new();
    d.sample();
    assert!(d.p().search_done);
    let pots = d.p().potential.len();
    assert!(pots >= 4, "three more tables with axes and one without: {pots}");
    assert!(d.p().potential.iter().any(|m| m.start == 0xB000 && m.cols == 14), "{:?}", d.p().potential.iter().map(|m| (m.start, m.cols, m.rows)).collect::<Vec<_>>());
    let first = d.p().potential[0].clone();
    d.typed("f");
    let g = d.g();
    assert_eq!(g.addr(g.cursor), first.start, "f lands on the first potential map");
    assert_eq!(g.cols, first.cols);
    assert_eq!(g.org, first.org);
    assert_eq!(g.sel_mode, SelMode::Rectangle);
    let maps = d.p().maps.len();
    let wins = d.app.children.len();
    d.typed("k");
    assert_eq!(d.p().maps.len(), maps + 1, "k makes a map");
    assert_eq!(d.app.children.len(), wins + 1, "and opens its window");
    let m = d.p().maps.last().expect("map").clone();
    assert_eq!((m.cols, m.rows, m.start), (first.cols, first.rows, first.start));
    let new_win = d.app.active_child().expect("window");
    assert_eq!(new_win.map_uid, Some(m.uid));
    assert_eq!(m.x.src, AxisSrc::Rom, "axes before the values are recognised");
    assert_eq!(d.p().potential.len(), pots - 1, "it isn't only potential any more");
}

#[test]
fn plus_minus_undo_and_original() {
    let mut d = Driver::new();
    d.sample();
    d.typed("f");
    d.key(Key::Delete, Modifiers::NONE);
    assert!(d.g().sel.is_none(), "Delete removes the selection");
    let g = d.g();
    let a = g.addr(g.cursor);
    let before = g.org.read(d.p().data(), a, false).unwrap();
    d.typed("+");
    assert_eq!(g.org.read(d.p().data(), a, false), Some(before + 1.0));
    d.typed("-");
    d.typed("-");
    assert_eq!(g.org.read(d.p().data(), a, false), Some(before - 1.0));
    d.key(Key::Z, CTRL);
    assert_eq!(g.org.read(d.p().data(), a, false), Some(before), "Ctrl+Z");
    d.key(Key::Y, CTRL);
    assert_eq!(g.org.read(d.p().data(), a, false), Some(before - 1.0), "Ctrl+Y");
    d.key(Key::F11, Modifiers::NONE);
    assert_eq!(g.org.read(d.p().data(), a, false), g.org.read(d.p().ref_data(), a, false), "F11 restores");
}

#[test]
fn enter_types_a_new_value() {
    let mut d = Driver::new();
    d.sample();
    d.typed("f");
    d.key(Key::Delete, Modifiers::NONE);
    d.cmd(Cmd::Radix(10));
    let g = d.g();
    let a = g.addr(g.cursor);
    d.key(Key::Enter, Modifiers::NONE);
    assert!(d.g().inplace.is_some(), "Enter opens the editor");
    d.frame(vec![Event::Key { key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: CTRL }], CTRL);
    d.frames(2);
    d.typed("1234");
    assert!(d.g().inplace.as_deref().is_some_and(|t| t.ends_with("1234")), "typed into the editor: {:?}", d.g().inplace);
    d.app.with_grid(|g| g.inplace = Some("1234".into()));
    d.key(Key::Enter, Modifiers::NONE);
    assert!(d.g().inplace.is_none(), "Enter closes it");
    assert_eq!(g.org.read(d.p().data(), a, false), Some(1234.0));
}

#[test]
fn number_key_starts_editing() {
    let mut d = Driver::new();
    d.sample();
    d.typed("7");
    assert_eq!(d.g().inplace.as_deref(), Some("7"));
    d.key(Key::Escape, Modifiers::NONE);
    assert!(d.g().inplace.is_none(), "Escape cancels");
}

#[test]
fn mouse_click_and_drag_select() {
    let mut d = Driver::new();
    d.sample();
    let id = d.app.active.unwrap();
    d.click(d.cell_pos(0x25));
    assert_eq!(d.g().cursor, 0x25, "a click puts the cursor on the cell");
    assert!(d.g().sel.is_none());
    assert_eq!(d.app.active, Some(id));
    d.typed("m");
    assert_eq!(d.g().cols, 17, "keys still reach the window after a click");
    d.typed("w");
    d.drag(d.cell_pos(0x12), d.cell_pos(0x35));
    let g = d.g();
    let s = g.sel.expect("a drag selects");
    assert_eq!((s.anchor, s.head), (0x12, 0x35));
    let cells = g.selected_cells(0x40000);
    assert_eq!(cells.first(), Some(&0x12));
    assert_eq!(cells.last(), Some(&0x35));
    // A second drag adds an area.
    d.drag(d.cell_pos(0x82), d.cell_pos(0x84));
    let g = d.g();
    assert!(g.is_selected(0x20) && g.is_selected(0x83) && !g.is_selected(0x60));
}

#[test]
fn right_click_on_cell_keeps_selection_and_opens_a_menu() {
    let mut d = Driver::new();
    d.sample();
    d.drag(d.cell_pos(0x12), d.cell_pos(0x15));
    let pos = d.cell_pos(0x13);
    d.frame(vec![Event::PointerMoved(pos)], Modifiers::NONE);
    d.press_at(pos, true, PointerButton::Secondary, Modifiers::NONE);
    d.press_at(pos, false, PointerButton::Secondary, Modifiers::NONE);
    d.frames(3);
    assert!(d.g().sel.is_some(), "the selection stays for the menu");
}

#[test]
fn shift_arrows_select_and_home_end_move() {
    let mut d = Driver::new();
    d.sample();
    d.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!(d.g().cursor, 1);
    d.key(Key::ArrowDown, SHIFT);
    let g = d.g();
    assert_eq!(g.cursor, 17);
    assert_eq!(g.sel.map(|s| (s.anchor, s.head)), Some((1, 17)));
    d.key(Key::Home, CTRL);
    assert_eq!(d.g().cursor, 0);
    d.key(Key::End, CTRL);
    assert_eq!(d.g().cursor, 0x40000 - 1);
    assert!(d.g().top_row > 0, "the view follows the cursor");
}

#[test]
fn columns_view_modes_and_diff_keys() {
    let mut d = Driver::new();
    d.sample();
    d.typed("m");
    assert_eq!(d.g().cols, 17);
    d.typed("w");
    d.typed("w");
    assert_eq!(d.g().cols, 15);
    d.typed("z");
    assert_eq!(d.g().cols, 16);
    d.typed("T");
    assert_eq!(d.g().mode, ViewMode::TwoD);
    d.typed("T");
    assert_eq!(d.g().mode, ViewMode::ThreeD);
    d.typed("t");
    d.typed("t");
    assert_eq!(d.g().mode, ViewMode::Text);
    // The sample version differs from its original.
    d.typed("e");
    let first = d.g().cursor;
    let a = d.g().addr(first);
    assert!(d.p().is_changed(DataOrg::Byte, a, false));
    d.typed("n");
    assert!(d.g().cursor > first);
    d.typed("v");
    assert_eq!(d.g().cursor, first);
    d.typed("l");
    assert!(d.g().cursor > first);
}

#[test]
fn every_view_of_a_map_draws_with_input() {
    let mut d = Driver::new();
    d.sample();
    let uid = d.p().maps[0].uid;
    let pid = d.p().id;
    d.app.open_map(pid, uid);
    d.frames(3);
    for m in [ViewMode::Text, ViewMode::TwoD, ViewMode::ThreeD] {
        d.cmd(Cmd::Mode(m));
        let r = d.app.active_child().unwrap().rect;
        d.drag(r.center(), r.center() + Vec2::new(40.0, 25.0));
        d.click(r.center() + Vec2::new(-30.0, 10.0));
        assert_eq!(d.g().mode, m);
    }
    // In 3d a drag away from the values turns the view.
    let before = d.g().rot;
    let r = d.app.active_child().unwrap().rect;
    d.drag(r.left_bottom() + Vec2::new(30.0, -40.0), r.left_bottom() + Vec2::new(120.0, -40.0));
    assert_ne!(d.g().rot, before);
}

#[test]
fn every_dialog_opens_and_closes() {
    let mut d = Driver::new();
    d.sample();
    d.cmd(Cmd::SelScreen);
    let uid = d.p().maps[0].uid;
    let pid = d.p().id;
    for c in [
        Cmd::NewProject,
        Cmd::OpenProject,
        Cmd::OpenVersion,
        Cmd::PropsProject,
        Cmd::PropsVersion,
        Cmd::ExportFile,
        Cmd::ImportFile,
        Cmd::ImportMaps,
        Cmd::ImportChanges,
        Cmd::UndoMore,
        Cmd::AsText,
        Cmd::ChangeAbs,
        Cmd::ChangeRel,
        Cmd::ChangeEdit,
        Cmd::Checksums,
        Cmd::PropsWindow,
        Cmd::SwapLines,
        Cmd::ChooseRef,
        Cmd::ConnectWindows,
        Cmd::SelCreate,
        Cmd::Replace,
        Cmd::MapSearchConfig,
        Cmd::AxisProfiles,
        Cmd::GoTo,
        Cmd::CommentEdit,
        Cmd::Config,
        Cmd::ProjectComment,
        Cmd::Help("start"),
        Cmd::Help("keys"),
        Cmd::Tip,
        Cmd::About,
    ] {
        d.cmd(c.clone());
        assert!(d.app.modal.is_some(), "{c:?} should open a dialog");
        d.key(Key::Escape, Modifiers::NONE);
        assert!(d.app.modal.is_none(), "{c:?} should close with Escape");
    }
    for tab in 0..7 {
        d.app.modal = Some(Modal::Config { tab });
        d.frames(3);
    }
    d.app.modal = None;
    d.app.open_map(pid, uid);
    d.frames(2);
    for tab in 0..4 {
        d.cmd(Cmd::PropsWindow);
        if let Some(Modal::MapProps { tab: t, .. }) = &mut d.app.modal {
            *t = tab;
        }
        d.frames(3);
        d.app.modal = None;
    }
    d.cmd(Cmd::ParallelMaps);
    assert!(matches!(d.app.modal, Some(Modal::Parallel { .. })));
    d.key(Key::Escape, Modifiers::NONE);
}

#[test]
fn floating_windows_toggle() {
    let mut d = Driver::new();
    d.sample();
    d.typed("o");
    assert!(d.app.show_overview);
    d.typed("p");
    assert!(d.app.show_preview);
    d.typed("u");
    assert!(d.app.show_diff);
    d.typed("s");
    assert!(d.app.find.open);
    d.key(Key::Tab, Modifiers::NONE);
    assert!(d.app.floats_hidden);
    d.key(Key::Tab, Modifiers::NONE);
    assert!(!d.app.floats_hidden);
    d.frames(3);
}

#[test]
fn search_goes_to_the_hit_and_selects_it() {
    let mut d = Driver::new();
    d.sample();
    d.app.find.text = "SAMPLE DATA".into();
    d.app.find.tab = crate::search::Radix::Text;
    d.key(Key::F3, Modifiers::NONE);
    let g = d.g();
    assert_eq!(g.addr(g.cursor), 0x2100);
    assert!(g.sel.is_some());
    d.key(Key::F3, Modifiers::NONE);
    assert_eq!(d.g().addr(d.g().cursor), 0x2100, "no second hit: it stays");
}

#[test]
fn go_to_dialog_takes_typed_text() {
    let mut d = Driver::new();
    d.sample();
    d.key(Key::G, CTRL);
    assert!(matches!(d.app.modal, Some(Modal::GoTo { .. })));
    if let Some(Modal::GoTo { text, .. }) = &mut d.app.modal {
        *text = "1000".into();
    }
    d.frames(2);
    d.key(Key::Enter, Modifiers::NONE);
    assert!(d.app.modal.is_none());
    assert_eq!(d.g().addr(d.g().cursor), 0x1000);
    assert_eq!(d.app.list.history.first().map(String::as_str), Some("0"));
}

#[test]
fn change_relative_applies_and_repeats_with_f9() {
    let mut d = Driver::new();
    d.sample();
    d.typed("f");
    d.key(Key::Delete, Modifiers::NONE);
    let g = d.g();
    let a = g.addr(g.cursor);
    let v = g.org.read(d.p().data(), a, false).unwrap();
    d.cmd(Cmd::ChangeRel);
    if let Some(Modal::ChangeRel { percent, w, .. }) = &mut d.app.modal {
        *percent = true;
        *w = "10".into();
    }
    d.frames(2);
    d.key(Key::Enter, Modifiers::NONE);
    assert!(d.app.modal.is_none());
    assert!(d.g().inplace.is_none(), "the Enter that closed the dialog stays out of the data window");
    let v1 = g.org.read(d.p().data(), a, false).unwrap();
    assert_eq!(v1, (v * 1.1).round());
    d.key(Key::F9, Modifiers::NONE);
    let v2 = g.org.read(d.p().data(), a, false).unwrap();
    assert_eq!(v2, (v1 * 1.1).round(), "F9 repeats it");
}

#[test]
fn window_buttons_and_tile() {
    let mut d = Driver::new();
    d.sample();
    let id = d.app.active.unwrap();
    d.cmd(Cmd::NewWindow);
    assert_eq!(d.app.children.len(), 2);
    d.cmd(Cmd::Tile);
    let (a, b) = (d.app.children[0].rect, d.app.children[1].rect);
    assert!(a.intersect(b).area() < 1.0, "tiled windows don't overlap");
    // Close button on the first window, in its title bar.
    let close = Pos2::new(a.right() - 1.0 - 84.0 + 2.0 * 28.0 + 13.0, a.top() + 1.0 + 3.0 + 9.0);
    d.click(close);
    assert_eq!(d.app.children.len(), 1);
    assert!(d.app.children.iter().all(|c| c.id != id));
}

#[test]
fn closing_a_changed_project_asks_first() {
    let mut d = Driver::new();
    d.sample();
    d.typed("f");
    d.typed("+");
    assert!(d.p().dirty_data);
    d.cmd(Cmd::CloseProject);
    assert!(matches!(d.app.modal, Some(Modal::SaveChanges { .. })));
    assert_eq!(d.app.projects.len(), 1, "nothing is lost yet");
    d.key(Key::Escape, Modifiers::NONE);
    assert_eq!(d.app.projects.len(), 1);
}

#[test]
fn closing_a_clean_project_removes_its_windows() {
    let mut d = Driver::new();
    d.sample();
    d.cmd(Cmd::CloseProject);
    assert!(d.app.projects.is_empty());
    assert!(d.app.children.is_empty());
    d.frames(3);
}

#[test]
fn import_a_file_and_a_second_version_then_export_round_trip() {
    let dir = std::env::temp_dir().join("als_drive_test");
    std::fs::create_dir_all(&dir).unwrap();
    let f1 = dir.join("one.bin");
    let f2 = dir.join("two.bin");
    let mut bytes = vec![0x11u8; 0x1000];
    std::fs::write(&f1, &bytes).unwrap();
    bytes[5] = 0x99;
    std::fs::write(&f2, &bytes).unwrap();
    let mut d = Driver::new();
    let ctx = d.ctx.clone();
    d.app.import_path(&f1, None, &ctx);
    d.frames(3);
    assert_eq!(d.app.projects.len(), 1);
    assert_eq!(d.app.children.len(), 1);
    assert_eq!(d.p().versions[0].name, "Original");
    let pid = d.p().id;
    d.app.import_path(&f2, Some(pid), &ctx);
    d.frames(3);
    assert_eq!(d.p().versions.len(), 2, "a second file on a project is a new version");
    assert_eq!(d.p().current, 1);
    assert!(d.p().is_changed(DataOrg::Byte, 5, false));
    assert!(!d.p().is_changed(DataOrg::Byte, 6, false));
    let text = crate::formats::write_ihex(d.p().data(), 0, 32);
    let hex = dir.join("three.hex");
    std::fs::write(&hex, text).unwrap();
    d.app.import_path(&hex, Some(pid), &ctx);
    d.frames(2);
    assert!(matches!(d.app.modal, Some(Modal::HexImport { .. })), "hex files ask for the project size");
    d.key(Key::Escape, Modifiers::NONE);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn checksum_added_in_the_project_corrects_itself_after_a_keypress() {
    let mut d = Driver::new();
    d.sample();
    {
        let p = d.app.current_project_mut().unwrap();
        let mut c = Checksum { name: "t".into(), from: 0x8000, to: 0x9FFF, org: DataOrg::Byte, target: 0x7FFF, target_end: 0x7FFF, auto: true, ..Checksum::default() };
        let cur = p.data().to_vec();
        crate::checksum::init_expected(&mut c, &cur);
        p.checksums.push(c);
    }
    d.app.with_grid(|g| g.cursor = 0x8100);
    d.frames(2);
    d.typed("+");
    let p = d.p();
    assert!(crate::checksum::is_ok(p.data(), &p.checksums[0]));
    assert_eq!(crate::checksum::status_text(p), "Sums: 1 ok");
}

#[test]
fn map_list_click_selects_and_double_click_opens() {
    let mut d = Driver::new();
    d.sample();
    let wins = d.app.children.len();
    let uid = d.p().maps[0].uid;
    let pid = d.p().id;
    d.cmd(Cmd::ListOpen(pid, uid, crate::shell::RowKind::Map));
    assert_eq!(d.app.children.len(), wins + 1);
    d.cmd(Cmd::ListOpen(pid, uid, crate::shell::RowKind::Map));
    assert_eq!(d.app.children.len(), wins, "a second time closes it");
    let pot = d.p().potential[0].uid;
    let n = d.p().potential.len();
    d.cmd(Cmd::ListOpen(pid, pot, crate::shell::RowKind::Potential));
    assert_eq!(d.p().potential.len(), n - 1, "keeping a potential map moves it");
    d.cmd(Cmd::ListRemove(pid, uid, crate::shell::RowKind::Map));
    assert!(d.p().maps.iter().all(|m| m.uid != uid));
}

#[test]
fn map_list_edge_drags_wider_and_narrower() {
    let mut d = Driver::new();
    d.sample();
    assert_eq!(d.app.cfg.list_width, 234.0);
    let window = d.app.active_child().unwrap().rect;
    // The grip is the last few points of the list, away from the fold tab in the middle.
    d.drag(Pos2::new(231.0, 200.0), Pos2::new(431.0, 210.0));
    assert!((d.app.cfg.list_width - 434.0).abs() < 4.0, "width {}", d.app.cfg.list_width);
    assert!(d.app.list.visible, "dragging the edge doesn't fold the list");
    let shift = d.app.active_child().unwrap().rect.left() - window.left();
    assert!((shift - 200.0).abs() < 4.0, "the data window moves along with the workspace, not under the list: {shift}");
    let edge = d.app.cfg.list_width - 3.0;
    d.drag(Pos2::new(edge, 200.0), Pos2::new(20.0, 200.0));
    assert_eq!(d.app.cfg.list_width, crate::shell::LIST_MIN);
}

#[test]
fn map_list_columns_drag_wider_and_narrower() {
    let mut d = Driver::new();
    d.sample();
    assert_eq!((d.app.cfg.list_addr_w, d.app.cfg.list_size_w), (96.0, 40.0));
    // The list head sits under the filter row; the name column begins 96 points in.
    d.drag(Pos2::new(99.0, 215.0), Pos2::new(120.0, 215.0));
    assert!((d.app.cfg.list_addr_w - 117.0).abs() < 2.0, "name edge {}", d.app.cfg.list_addr_w);
    // The size column's edge is 40 points from the right end of the head.
    d.drag(Pos2::new(187.0, 215.0), Pos2::new(197.0, 215.0));
    assert!((d.app.cfg.list_size_w - 30.0).abs() < 2.0, "size edge {}", d.app.cfg.list_size_w);
    // Neither column can be dragged away to nothing.
    d.drag(Pos2::new(120.0, 215.0), Pos2::new(5.0, 215.0));
    assert_eq!(d.app.cfg.list_addr_w, crate::shell::LIST_ADDR_MIN);
    assert!(!d.app.list.floating, "dragging an edge isn't a double-click on the head");
}

#[test]
fn height_colours_switch_on_and_draw_in_every_view() {
    let mut d = Driver::new();
    d.sample();
    assert!(!d.app.cfg.height_colors);
    d.cmd(Cmd::HeightColors);
    assert!(d.app.cfg.height_colors);
    let (pid, uid) = (d.p().id, d.p().maps[0].uid);
    d.cmd(Cmd::ListOpen(pid, uid, crate::shell::RowKind::Map));
    for mode in [crate::model::ViewMode::Text, crate::model::ViewMode::TwoD, crate::model::ViewMode::ThreeD] {
        d.cmd(Cmd::Mode(mode));
        assert_eq!(d.g().mode, mode);
    }
    d.cmd(Cmd::HeightColors);
    assert!(!d.app.cfg.height_colors);
}

#[test]
fn window_colours_follow_the_choice() {
    use crate::config::Colours;
    let mut d = Driver::new();
    assert!(!d.app.wants_dark(), "light on its own, with no TunerPro to follow");
    d.app.tunerpro.dark = Some(true);
    assert!(d.app.wants_dark(), "dark when TunerPro is");
    d.cmd(Cmd::Colours(Colours::Light));
    assert!(!d.app.wants_dark());
    d.app.tunerpro.dark = None;
    d.app.cfg.colours = Colours::Dark;
    assert!(d.app.wants_dark());
}

#[test]
fn protected_memory_asks_and_obeys() {
    let mut d = Driver::new();
    d.sample();
    d.typed("f");
    d.key(Key::Delete, Modifiers::NONE);
    let g = d.g();
    let a = g.addr(g.cursor);
    let before = g.org.read(d.p().data(), a, false).unwrap();
    d.app.current_project_mut().unwrap().protect_outside = true;
    d.typed("+");
    assert!(matches!(d.app.modal, Some(Modal::Protect { .. })), "it asks instead of changing");
    assert_eq!(g.org.read(d.p().data(), a, false), Some(before), "nothing changed yet");
    d.key(Key::Escape, Modifiers::NONE);
    assert_eq!(g.org.read(d.p().data(), a, false), Some(before), "Escape cancels the operation");
    // A covered address goes straight through.
    {
        let p = d.app.current_project_mut().unwrap();
        p.checksums.push(Checksum { from: a, to: a + 1, org: DataOrg::Byte, display_only: true, ..Checksum::default() });
    }
    d.typed("+");
    assert!(d.app.modal.is_none());
    assert_eq!(g.org.read(d.p().data(), a, false), Some(before + 1.0));
}
