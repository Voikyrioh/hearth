//! Pastille de l'icône de la zone de notification : dessin et accord avec les jetons de
//! `src/styles/tokens.css` (si un jeton change, ce test casse).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use hearth_desktop_lib::badge::{COLOR_CRIT, COLOR_OK, COLOR_RING, COLOR_WARN, color, paint_badge};
use hearth_desktop_lib::presence::TrayStatus;

fn token(css: &str, name: &str) -> [u8; 3] {
    let line = css
        .lines()
        .find(|line| line.trim_start().starts_with(&format!("--{name}:")))
        .unwrap_or_else(|| panic!("jeton --{name} absent de tokens.css"));
    let hex = line.split('#').nth(1).unwrap().trim_end_matches(';').trim();
    assert_eq!(hex.len(), 6, "--{name} : {hex}");
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
    [byte(0), byte(2), byte(4)]
}

#[test]
fn the_badge_colors_follow_the_design_tokens() {
    let css = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../src/styles/tokens.css"
    ))
    .unwrap();
    assert_eq!(COLOR_OK, token(&css, "ok"));
    assert_eq!(COLOR_WARN, token(&css, "warn"));
    assert_eq!(COLOR_CRIT, token(&css, "crit"));
    assert_eq!(COLOR_RING, token(&css, "bg"));
}

#[test]
fn every_status_but_idle_has_its_own_color() {
    assert_eq!(color(TrayStatus::Idle), None);
    assert_eq!(color(TrayStatus::Connected), Some(COLOR_OK));
    assert_eq!(color(TrayStatus::Warning), Some(COLOR_WARN));
    assert_eq!(color(TrayStatus::Critical), Some(COLOR_CRIT));
}

#[test]
fn the_badge_is_painted_in_the_bottom_right_corner_only() {
    let (w, h) = (16_u32, 16_u32);
    let blank = vec![0_u8; (w * h * 4) as usize];
    let painted = paint_badge(&blank, w, h, TrayStatus::Critical);
    let at = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [painted[i], painted[i + 1], painted[i + 2], painted[i + 3]]
    };
    assert_eq!(at(12, 12), [0xff, 0x5a, 0x5a, 0xff], "pastille");
    assert_eq!(at(1, 1), [0, 0, 0, 0], "le reste n'est pas touché");
    // Neutre : l'image est rendue telle quelle.
    assert_eq!(paint_badge(&blank, w, h, TrayStatus::Idle), blank);
    // Taille incohérente : rendue telle quelle, sans panique.
    assert_eq!(
        paint_badge(&blank[..10], w, h, TrayStatus::Critical),
        &blank[..10]
    );
}

#[test]
fn the_three_colors_differ() {
    let blank = vec![0_u8; 16 * 16 * 4];
    let a = paint_badge(&blank, 16, 16, TrayStatus::Connected);
    let b = paint_badge(&blank, 16, 16, TrayStatus::Warning);
    let c = paint_badge(&blank, 16, 16, TrayStatus::Critical);
    assert_ne!(a, b);
    assert_ne!(b, c);
    assert_ne!(a, c);
}
