---
id: BR-RESIL-004
domaine: RESIL
titre: Au-delà de 30 secondes de coupure, l'état du lien est « Hors ligne » : bandeau avec l'heure du dernier contact et « Réessayer maintenant »
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-004), ADR-0007, HRT-09
maj: 2026-10-05
---

# BR-RESIL-004 — Au-delà de 30 secondes de coupure, l'état du lien est « Hors ligne » : bandeau avec l'heure du dernier contact et « Réessayer maintenant »

## Règle
À partir de 30 s de coupure, l'état passe à `Offline` ; les tentatives continuent sans fin. L'événement d'état porte l'heure du dernier contact (`last_contact_at`) pour le bandeau « Dernier contact à {heure} » et la date de la prochaine tentative (`next_retry_at`). « Réessayer maintenant » depuis `Offline` affiche `Reconnecting` le temps de la tentative, puis revient à `Offline` si elle échoue.

Quand l'état du lien est « Hors ligne », un bandeau non modal affiche « Serveur hors ligne. Dernier contact à {heure}. Nouvelle tentative automatique en cours. » (heure locale `14h23`) et le bouton « Réessayer maintenant », qui demande une tentative immédiate (BR-RESIL-005). Sans contact connu, la phrase omet l'heure. Le bandeau disparaît au retour de « Connecté » ; en « Reconnexion… » il n'est pas affiché.

## Application (code)
- Bibliothèque :
  - `crates/hearth-link/src/domain/state.rs::LinkMachine::derive_down` (`LinkState::Offline`), `LinkMachine::status` (`Status::last_contact`, `Status::next_retry_at`), `LinkMachine::on_trigger` (`Input::RetryNow`).
- Interface :
  - `apps/desktop/src/components/organisms/OfflineBanner.vue` ; `apps/desktop/src/layouts/ServerLayout.vue` (affiché si l'état est `offline`, `retry` → `link.retryNow`).
  - `apps/desktop/src/stores/link.ts::retryNow` → `LinkBridge.retryNow`.

## Vérification
- Bibliothèque : `domain::state::tests::row03_connected_cut_over_30s_shows_offline` (29 999 / 30 000 ms), `::row05_reconnecting_with_no_answer_for_30s_goes_offline`, `::row06_offline_retry_now_shows_reconnecting_and_starts_an_attempt`, `::row07_offline_link_back_automatically_is_connected`, `::a_late_tick_dates_the_state_at_the_threshold_not_at_the_late_instant`.
- Bibliothèque, intégration : `tests/fault_proxy.rs::a_long_cut_goes_offline_then_comes_back`, `::retry_now_forces_an_attempt_without_waiting`.
- Interface : `organisms.test.ts::OfflineBanner`, `shell.test.ts::goes connected -> reconnecting -> offline -> back`, `shell.test.ts::« Réessayer maintenant » asks the bridge to retry that server`, `e2e/shell.spec.ts`.
- Coquille : `apps/desktop/src-tauri/tests/offline.rs``::a_long_cut_shows_reconnecting_then_offline_notifies_once_and_turns_the_icon_red_then_green` (« Reconnexion… » puis « Hors ligne » reçus à l'écran).
- Interface : `apps/desktop/e2e/offline.spec.ts` (« hors ligne : bandeau, données périmées, retour du lien »).

## Cas limites
- Une tentative lancée par « Réessayer maintenant » qui échoue remet `Offline` avec la date d'origine (`since` inchangé : le serveur est resté hors ligne sans interruption).
- L'empreinte changée et les versions incompatibles s'affichent `Offline` avec `blocked` renseigné et aucune tentative planifiée (voir BR-CONN-003).
- Texte exact du bandeau : `fr.ts` clés `link.offlineBanner`, `link.offlineBannerNoContact`, `link.retryNow`.

## Règles liées
- BR-RESIL-001, BR-RESIL-005.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — création de la partie interface (HRT-09, session 2026-10-04-hearth-creation). Portée par l'interface ; le calcul des états est dans `hearth-link` (ADR-0007).
- 2026-10-05 — fiches HRT-07 et HRT-09 réunies (fusion de main dans feat/HRT-07-link).
- 2026-10-05 : tests de la coquille et de l'écran (HRT-12).
