// Pas de console Windows en version livrée.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    hearth_desktop_lib::run();
}
