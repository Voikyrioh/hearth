---
id: BR-RESIL-015
domaine: RESIL
titre: Notifications système optionnelles, une par minute et par serveur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-015)
maj: 2026-10-05
---

# BR-RESIL-015 — Notifications système optionnelles, une par minute et par serveur

## Règle
Les notifications Windows signalent le passage à « Hors ligne » ou le retour à « Connecté », sont désactivables dans les réglages, et sont limitées à UNE par minute et par serveur (texte de la spec, toutes natures confondues). Ce que la limite retient n'est pas perdu : seule la dernière situation compte, elle est annoncée à l'échéance, une fois, avec le nombre de changements absorbés en plus ; une situation déjà annoncée n'est jamais répétée. Les notifications Windows ne signalent QUE ces deux changements d'état : une panne qui dure ne produit rien de plus, quelle que soit sa durée (24 h ou 7 jours éteint : une notification pour la panne, une au retour). Les échecs de reconnexion répétés ne notifient jamais le système : leur compteur vit dans les notifications discrètes de l'application (BR-RESIL-018). Elles s'appuient sur l'événement d'état de la bibliothèque ; la limitation et le réglage sont de la coquille Tauri. Réglage « Notifier quand un serveur devient hors ligne ou revient » : activé par défaut.

## Application (code)
- Données fournies par `crates/hearth-link/src/domain/state.rs::LinkMachine::status` et par les événements d'état du `LinkManager` (`link.rs::LinkRuntime::relay` les passe à l'observateur).
- Règle pure : `apps/desktop/src-tauri/src/presence.rs::NotificationGate::{observe, poll}` (fenêtre `NOTIFY_WINDOW_MS` = 60 s par serveur).
- Colle : `apps/desktop/src-tauri/src/alerts.rs::Alerts` (observateur d'états `link.rs::StateObserver`, minuteur de 5 s dans `lib.rs::install_link`), notification par `tray.rs::TauriNotifier` (greffon de notifications).
- Réglage : `settings.rs::{notify_on_link_change, set_notify_on_link_change}`, commandes `get_notify_on_link_change` / `set_notify_on_link_change`, case de `pages/Settings.vue` (section « Notifications »).

## Vérification
- Règle : `apps/desktop/src-tauri/tests/presence.rs` (une seule fenêtre d'une minute par serveur, retour retenu puis annoncé, dix coupures en 40 s, retenue annoncée avec son compte, situation déjà annoncée, serveur éteint 24 h puis 7 jours : nombre de notifications borné et indépendant de la durée, chaque serveur son compte, nouvelle panne : le cycle repart, indépendance des serveurs, mémoire bornée sur trois jours).
- Colle : `apps/desktop/src-tauri/tests/alerts.rs` (textes, minuteur, réglage coupé : plus rien ne part, serveurs indépendants). Contre un vrai agent : `apps/desktop/src-tauri/tests/offline.rs``::a_long_cut_shows_reconnecting_then_offline_notifies_once_and_turns_the_icon_red_then_green`.
- Interface : `pages/Settings.test.ts`, `apps/desktop/e2e/offline.spec.ts` (« réglages »).

## Cas limites
- Sans objet pour la bibliothèque.

## Règles liées
- BR-RESIL-002 à BR-RESIL-005.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 : implémentée (HRT-12) : limiteur, agrégation, réglage, notification système.
- 2026-10-05 : remise au texte de la spec (revue HRT-12) : UNE notification par minute et par serveur, toutes natures confondues ; paliers d'échecs.
- 2026-10-06 : plus aucune notification système pour les échecs répétés (revue round 2 : un serveur éteint la nuit en aurait produit ~570 par jour) ; invariant borné testé sur 24 h et 7 jours.
