//! Icône de la zone de notification : clic gauche = ouvrir, menu « Ouvrir
//! Hearth » / « Quitter » (BR-CLIENT-011).

use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Runtime};

use tauri_plugin_notification::NotificationExt as _;

use crate::alerts::{Notifier, TrayPort};
use crate::domain::{MENU_OPEN, MENU_QUIT, TrayAction, tray_action};
use crate::presence::{TrayIcon as IconState, TrayStatus};
use crate::{texts, tray_icons, window};

/// Montre l'état du lien dans la zone de notification : une image par état (l'âtre, et dedans une
/// flamme, un contour, une barre, un point d'exclamation ou une croix ; BR-RESIL-016, HRT-19),
/// l'infobulle disant quel serveur et quel état. Un échec est journalisé, jamais fatal.
pub struct TauriTray<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> TrayPort for TauriTray<R> {
    fn show_icon(&self, icon: IconState) {
        let Some(tray) = self.0.tray_by_id(TRAY_ID) else {
            return;
        };
        match Image::from_bytes(tray_icons::png(icon, icon_size(&self.0))) {
            Ok(image) => {
                if let Err(error) = tray.set_icon(Some(image)) {
                    tracing::warn!(%error, "icône de la zone de notification non mise à jour");
                }
            }
            Err(error) => tracing::warn!(%error, "image de l'icône illisible"),
        }
    }

    fn show(&self, _status: TrayStatus, tooltip: &str) {
        let Some(tray) = self.0.tray_by_id(TRAY_ID) else {
            return;
        };
        if let Err(error) = tray.set_tooltip(Some(tooltip)) {
            tracing::warn!(%error, "infobulle de la zone de notification non mise à jour");
        }
    }
}

/// Taille d'image la plus proche de l'échelle de l'écran principal (100 % : 16 px).
fn icon_size<R: Runtime>(app: &AppHandle<R>) -> u32 {
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map_or(1.0, |monitor| monitor.scale_factor());
    tray_icons::pick_size(scale)
}

/// Notifications système du lien, par le greffon de notifications.
pub struct TauriNotifier<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> Notifier for TauriNotifier<R> {
    fn notify(&self, title: &str, body: &str) {
        if let Err(error) = self
            .0
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
        {
            tracing::warn!(%error, "notification du lien refusée");
        }
    }
}

/// Identifiant de l'icône (pour la retirer si le démarrage échoue ensuite).
pub const TRAY_ID: &str = "hearth";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, MENU_OPEN, texts::MENU_OPEN_LABEL, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, texts::MENU_QUIT_LABEL, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(tray_icons::png(
            IconState::NoServer,
            icon_size(app),
        ))?)
        .tooltip(texts::APP_NAME)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(on_tray_icon_event)
        .build(app)?;
    Ok(())
}

fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    match tray_action(event.id().as_ref()) {
        Some(TrayAction::Open) => window::show_main(app),
        Some(TrayAction::Quit) => app.exit(0),
        None => {}
    }
}

fn on_tray_icon_event<R: Runtime>(tray: &TrayIcon<R>, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        window::show_main(tray.app_handle());
    }
}
