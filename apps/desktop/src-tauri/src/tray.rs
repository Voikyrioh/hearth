//! Icône de la zone de notification : clic gauche = ouvrir, menu « Ouvrir
//! Hearth » / « Quitter » (BR-CLIENT-011).

use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Runtime};

use tauri_plugin_notification::NotificationExt as _;

use crate::alerts::{Notifier, TrayPort};
use crate::domain::{MENU_OPEN, MENU_QUIT, TrayAction, tray_action};
use crate::presence::{TrayStatus, paint_badge};
use crate::{texts, window};

/// Flamme seule, une couleur (`hearth-logo-small.svg`), lisible à 16 px.
const TRAY_ICON: &[u8] = include_bytes!("../icons/tray.png");

/// Montre l'état du lien dans la zone de notification : la flamme et une pastille verte, orange ou
/// rouge (BR-RESIL-016), l'infobulle disant quel serveur et quel état. Un échec est journalisé,
/// jamais fatal.
pub struct TauriTray<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> TrayPort for TauriTray<R> {
    fn show(&self, status: TrayStatus, tooltip: &str) {
        let Some(tray) = self.0.tray_by_id(TRAY_ID) else {
            return;
        };
        match Image::from_bytes(TRAY_ICON) {
            Ok(base) => {
                let rgba = paint_badge(base.rgba(), base.width(), base.height(), status);
                let icon = Image::new_owned(rgba, base.width(), base.height());
                if let Err(error) = tray.set_icon(Some(icon)) {
                    tracing::warn!(%error, "icône de la zone de notification non mise à jour");
                }
            }
            Err(error) => tracing::warn!(%error, "icône de base illisible"),
        }
        if let Err(error) = tray.set_tooltip(Some(tooltip)) {
            tracing::warn!(%error, "infobulle de la zone de notification non mise à jour");
        }
    }
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
        .icon(Image::from_bytes(TRAY_ICON)?)
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
