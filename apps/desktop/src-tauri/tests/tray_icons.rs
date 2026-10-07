//! Icône de la zone de notification : une image par état et par taille, taille choisie selon
//! l'échelle de l'écran (HRT-19).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use hearth_desktop_lib::presence::TrayIcon;
use hearth_desktop_lib::tray_icons::{SIZES, pick_size, png};
use hearth_link::domain::state::LinkState;

const ALL: [TrayIcon; 6] = [
    TrayIcon::NoServer,
    TrayIcon::Connected,
    TrayIcon::Reconnecting,
    TrayIcon::Offline,
    TrayIcon::SessionExpired,
    TrayIcon::AccessRevoked,
];

fn dimensions(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "signature PNG");
    let be = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    (be(16), be(20))
}

#[test]
fn every_state_has_a_png_of_every_size() {
    for icon in ALL {
        for size in SIZES {
            assert_eq!(dimensions(png(icon, size)), (size, size), "{icon:?} {size}");
        }
    }
}

#[test]
fn the_six_images_of_a_size_are_all_different() {
    for size in SIZES {
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                assert_ne!(png(*a, size), png(*b, size), "{a:?} et {b:?} à {size}");
            }
        }
    }
}

#[test]
fn each_link_state_picks_its_own_icon() {
    let icons = [
        LinkState::Connected,
        LinkState::Reconnecting,
        LinkState::Offline,
        LinkState::SessionExpired,
        LinkState::AccessRevoked,
    ]
    .map(TrayIcon::of);
    assert_eq!(
        icons,
        [
            TrayIcon::Connected,
            TrayIcon::Reconnecting,
            TrayIcon::Offline,
            TrayIcon::SessionExpired,
            TrayIcon::AccessRevoked,
        ]
    );
}

#[test]
fn the_size_follows_the_screen_scale() {
    assert_eq!(pick_size(1.0), 16);
    assert_eq!(pick_size(1.25), 20);
    assert_eq!(pick_size(1.5), 24);
    assert_eq!(pick_size(1.8), 32);
    assert_eq!(pick_size(2.0), 32);
    assert_eq!(pick_size(3.0), 32);
    assert_eq!(pick_size(0.0), 16);
    assert_eq!(pick_size(f64::NAN), 16);
}

#[test]
fn an_unknown_size_falls_back_to_the_smallest() {
    assert_eq!(png(TrayIcon::Connected, 17), png(TrayIcon::Connected, 16));
}
