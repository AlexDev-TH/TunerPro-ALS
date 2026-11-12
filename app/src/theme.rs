//! The colour themes for the data windows.
use egui::Color32 as C;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    pub bg: [u8; 3],
    pub text: [u8; 3],
    pub map_bg: [u8; 3],
    pub map_text: [u8; 3],
    pub cpu_bg: [u8; 3],
    pub sel_bg: [u8; 3],
    pub sel_text: [u8; 3],
    pub right: [u8; 3],
    pub three_d: [u8; 3],
    pub changed: [u8; 3],
    pub increased: [u8; 3],
    pub decreased: [u8; 3],
    pub map_in_hex_bg: [u8; 3],
    pub map_in_hex_text: [u8; 3],
    pub rowmarker: [u8; 3],
    pub potential: [u8; 3],
    pub potential_tag: [u8; 3],
    pub comment_bg: [u8; 3],
    pub comment_text: [u8; 3],
    pub list_bg: [u8; 3],
    pub list_text: [u8; 3],
}

impl Default for Theme {
    fn default() -> Self {
        Theme::green()
    }
}

impl Theme {
    /// Green on black.
    pub fn green() -> Theme {
        Theme {
            name: "Green on black".into(),
            bg: [0, 0, 0],
            text: [0, 255, 0],
            map_bg: [0, 0, 0],
            map_text: [0, 255, 0],
            cpu_bg: [150, 0, 0],
            sel_bg: [0, 110, 0],
            sel_text: [255, 255, 255],
            right: [0, 255, 0],
            three_d: [0, 220, 0],
            changed: [255, 255, 255],
            increased: [224, 120, 120],
            decreased: [120, 160, 200],
            map_in_hex_bg: [0, 50, 0],
            map_in_hex_text: [0, 220, 0],
            rowmarker: [0, 90, 0],
            potential: [0, 200, 200],
            potential_tag: [0, 170, 170],
            comment_bg: [255, 255, 0],
            comment_text: [0, 0, 0],
            list_bg: [0, 0, 0],
            list_text: [0, 255, 0],
        }
    }
    /// White on blue.
    pub fn blue() -> Theme {
        Theme {
            name: "White on blue".into(),
            bg: [0, 0, 150],
            text: [190, 200, 220],
            map_bg: [0, 0, 170],
            map_text: [230, 235, 245],
            cpu_bg: [150, 0, 0],
            sel_bg: [60, 60, 220],
            sel_text: [255, 255, 255],
            right: [190, 195, 205],
            three_d: [230, 235, 245],
            changed: [255, 255, 255],
            increased: [224, 136, 136],
            decreased: [136, 165, 165],
            map_in_hex_bg: [70, 70, 200],
            map_in_hex_text: [200, 210, 225],
            rowmarker: [70, 70, 220],
            potential: [0, 230, 230],
            potential_tag: [0, 220, 220],
            comment_bg: [255, 255, 0],
            comment_text: [0, 0, 0],
            list_bg: [255, 255, 255],
            list_text: [0, 0, 120],
        }
    }
    pub fn light() -> Theme {
        Theme {
            name: "Black on white".into(),
            bg: [255, 255, 255],
            text: [40, 40, 40],
            map_bg: [250, 250, 240],
            map_text: [0, 0, 0],
            cpu_bg: [255, 170, 170],
            sel_bg: [170, 200, 240],
            sel_text: [0, 0, 0],
            right: [90, 110, 150],
            three_d: [20, 50, 120],
            changed: [180, 0, 0],
            increased: [255, 200, 200],
            decreased: [190, 220, 255],
            map_in_hex_bg: [225, 235, 250],
            map_in_hex_text: [20, 30, 90],
            rowmarker: [200, 200, 210],
            potential: [0, 140, 140],
            potential_tag: [150, 230, 230],
            comment_bg: [255, 255, 0],
            comment_text: [0, 0, 0],
            list_bg: [255, 255, 255],
            list_text: [0, 0, 0],
        }
    }
    pub fn presets() -> Vec<Theme> {
        vec![Theme::green(), Theme::blue(), Theme::light()]
    }
}

pub fn c(a: [u8; 3]) -> C {
    C::from_rgb(a[0], a[1], a[2])
}

/// A faded version for program code and empty areas: halfway to the background
/// (full 00FF00 becomes 007F00 on black).
pub fn pale(col: C, bg: C) -> C {
    let m = |a: u8, b: u8| ((a as u16 + b as u16) / 2) as u8;
    C::from_rgb(m(col.r(), bg.r()), m(col.g(), bg.g()), m(col.b(), bg.b()))
}
