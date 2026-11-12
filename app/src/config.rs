//! Settings kept between runs.
use crate::analysis::SearchRules;
use crate::model::{RightSide, ViewMode};
use crate::theme::Theme;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[serde(default)]
pub struct Client {
    pub name: String,
    pub path: String,
}

/// Which set of colours the program's own frame uses: menus, toolbars, window frames.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum Colours {
    /// Dark when TunerPro is showing its dark theme, light otherwise.
    #[default]
    LikeTunerPro,
    Light,
    Dark,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct Config {
    pub theme: Theme,
    pub user_theme: Theme,
    pub clients: Vec<Client>,
    pub client: usize,
    pub delete_sel_on_move: bool,
    pub visible_columns_only: bool,
    pub step8: u32,
    pub step16: u32,
    pub step32: u32,
    pub page_jump: usize,
    pub edit_shown_values: bool,
    pub number_keys_edit: bool,
    pub show_potential: bool,
    pub show_pale: bool,
    pub row_groups: bool,
    pub drag_2d: bool,
    pub drag_3d: bool,
    pub support_selection: bool,
    pub new_mode: ViewMode,
    pub new_radix: u32,
    pub new_right: RightSide,
    pub search_potential: bool,
    pub generate_overview: bool,
    pub load_open_projects: bool,
    pub auto_names: bool,
    pub namespace_from: u32,
    pub namespace_to: u32,
    pub show_props_on_import: bool,
    pub vehicle_on_import: bool,
    pub identical_on_import: bool,
    pub warn_big_selection: bool,
    pub show_cs_status: bool,
    pub ask_before_exit_tips: bool,
    pub rules: SearchRules,
    pub recent: Vec<String>,
    pub open_projects: Vec<String>,
    pub status_bar: bool,
    pub toolbar_view: bool,
    pub tips: bool,
    pub tip_index: usize,
    pub default_texts: Vec<String>,
    pub unit_presets: Vec<UnitPreset>,
    pub customers: Vec<String>,
    /// Width of the docked map list, as the person last dragged it.
    pub list_width: f32,
    /// Where the name column of the map list begins, and how wide its size column is.
    pub list_addr_w: f32,
    pub list_size_w: f32,
    pub colours: Colours,
    /// TunerPro's theme as last seen, for a start where its window can't be looked at
    /// (it is minimised, or the program was started on its own).
    pub host_dark: Option<bool>,
    /// Colour values by their height: blue for the lowest to red for the highest.
    pub height_colors: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default, PartialEq)]
#[serde(default)]
pub struct UnitPreset {
    pub name: String,
    pub unit: String,
    pub factor: f64,
    pub offset: f64,
    pub precision: u32,
}

pub fn documents() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|p| p.join("Documents"))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn data_home() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ALS")
}

impl Default for Config {
    fn default() -> Self {
        let base = documents().join("ALS");
        Config {
            theme: Theme::green(),
            user_theme: Theme { name: "User defined".into(), ..Theme::green() },
            clients: vec![Client { name: "Default".into(), path: base.join("Projects").to_string_lossy().to_string() }],
            client: 0,
            delete_sel_on_move: false,
            visible_columns_only: true,
            step8: 1,
            step16: 1,
            step32: 1,
            page_jump: 0x8000,
            edit_shown_values: true,
            number_keys_edit: true,
            show_potential: true,
            show_pale: true,
            row_groups: true,
            drag_2d: true,
            drag_3d: true,
            support_selection: false,
            new_mode: ViewMode::Text,
            new_radix: 16,
            new_right: RightSide::Bars,
            search_potential: true,
            generate_overview: true,
            load_open_projects: false,
            auto_names: true,
            namespace_from: 10000,
            namespace_to: 99999,
            show_props_on_import: false,
            vehicle_on_import: true,
            identical_on_import: false,
            warn_big_selection: true,
            show_cs_status: true,
            ask_before_exit_tips: false,
            rules: SearchRules::default(),
            recent: vec![],
            open_projects: vec![],
            status_bar: true,
            toolbar_view: true,
            tips: true,
            tip_index: 0,
            default_texts: vec![],
            unit_presets: vec![
                UnitPreset { name: "Bar".into(), unit: "bar".into(), factor: 0.001, offset: 0.0, precision: 3 },
                UnitPreset { name: "°C".into(), unit: "°C".into(), factor: 0.1, offset: -273.14, precision: 1 },
                UnitPreset { name: "1".into(), unit: "-".into(), factor: 1.0, offset: 0.0, precision: 0 },
                UnitPreset { name: "%".into(), unit: "%".into(), factor: 0.01, offset: 0.0, precision: 2 },
            ],
            customers: vec![],
            list_width: 234.0,
            list_addr_w: 96.0,
            list_size_w: 40.0,
            colours: Colours::LikeTunerPro,
            host_dark: None,
            height_colors: false,
        }
    }
}

impl Config {
    fn file() -> PathBuf {
        data_home().join("config.json")
    }
    pub fn load() -> Config {
        std::fs::read(Self::file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) {
        let _ = std::fs::create_dir_all(data_home());
        if let Ok(b) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(Self::file(), b);
        }
    }
    pub fn project_dir(&self) -> PathBuf {
        let c = self.clients.get(self.client).or_else(|| self.clients.first());
        c.map(|c| PathBuf::from(&c.path)).unwrap_or_else(|| documents().join("ALS").join("Projects"))
    }
    pub fn add_recent(&mut self, path: &str) {
        self.recent.retain(|r| r != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(8);
    }
}
