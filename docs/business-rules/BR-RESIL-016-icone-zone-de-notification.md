---
id: BR-RESIL-016
domaine: RESIL
titre: L'icône de la zone de notification reflète l'état du lien
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-016)
maj: 2026-10-05
---

# BR-RESIL-016 — L'icône de la zone de notification reflète l'état du lien

## Règle
L'icône change en temps réel avec l'état du lien du serveur affiché : la flamme garde sa forme, une pastille verte (« Connecté »), orange (« Reconnexion… », « Session expirée ») ou rouge (« Hors ligne », « Accès révoqué ») est posée dans un coin, et l'infobulle dit quel serveur et quel état. Sans serveur affiché (réglages), l'icône montre le pire état de tous les serveurs ; sans serveur enregistré, elle reste neutre. Elle s'abonne aux événements d'état de la bibliothèque ; l'affichage est de la coquille Tauri.

## Application (code)
- Données fournies par `crates/hearth-link/src/domain/state.rs::LinkMachine::status` et par les événements d'état du `LinkManager` (`link.rs::LinkRuntime::relay` les passe à l'observateur).
- Règle pure : `apps/desktop/src-tauri/src/presence.rs::{TrayStatus, LinkPresence::reflected, paint_badge}`.
- Colle : `alerts.rs::Alerts` (redessine seulement si l'état ou l'infobulle change), `tray.rs::TauriTray` (icône et infobulle du greffon tray) ; le serveur affiché vient de l'interface (`App.vue` → `LinkBridge::setDisplayedServer` → commande `set_displayed_server`).

## Vérification
- Règle : `apps/desktop/src-tauri/tests/presence.rs::the_tray_follows_the_displayed_server`, `::every_state_has_a_status_and_the_colors_follow_the_pill`, `::the_badge_is_painted_in_the_bottom_right_corner_only`.
- Colle : `apps/desktop/src-tauri/tests/alerts.rs::the_icon_follows_the_displayed_server_with_a_tooltip_and_is_only_redrawn_on_change`, `::removing_the_displayed_server_falls_back_then_goes_neutral`. Contre un vrai agent : `apps/desktop/src-tauri/tests/offline.rs`.

## Cas limites
- Sans objet pour la bibliothèque.

## Règles liées
- BR-RESIL-002 à BR-RESIL-005.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 : implémentée (HRT-12) : pastille de couleur sur l'icône, infobulle, suivi du serveur affiché. Reste à voir sur une vraie zone de notification Windows (rendu à 16 px, thème clair).
