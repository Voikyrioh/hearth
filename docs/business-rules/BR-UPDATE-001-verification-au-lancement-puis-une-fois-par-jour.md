---
id: BR-UPDATE-001
domaine: UPDATE
titre: Le client vérifie les mises à jour au lancement, puis une fois par jour au plus
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-001), HRT-16
maj: 2026-10-05
---

# BR-UPDATE-001 : Le client vérifie les mises à jour au lancement, puis une fois par jour au plus

## Règle
Au lancement, puis toutes les 24 h au plus, le client interroge le flux de versions (GitHub Releases du dépôt public). Jamais deux vérifications automatiques en moins de 24 h, qu'elles aient réussi ou non : l'instant de la tentative est écrit sur disque AVANT l'appel réseau (une application tuée pendant la vérification ne la refait pas). Un redémarrage dans la journée ne vérifie pas. La mémoire est un fichier du dossier de données (`update.json`), jamais la WebView : vider la WebView ne remet rien à zéro. Un « Vérifier maintenant » (BR-UPDATE-026) compte comme une vérification et repart pour 24 h.

## Application (code)
- `apps/desktop/src-tauri/src/update/domain.rs::{check_is_due, millis_until_due, CHECK_INTERVAL_MS}` : la règle, horloge injectée.
- `apps/desktop/src-tauri/src/update/service.rs::UpdateService::{check_if_due, tick, run_scheduler}` : première vérification 10 s après le lancement, puis un battement par heure sans réseau tant que la règle ne permet rien.
- `apps/desktop/src-tauri/src/update/store.rs::FileUpdateStore` : `update.json`, écriture atomique.

## Vérification
- `apps/desktop/src-tauri/tests/update_domain.rs` : `the_first_check_is_always_allowed`, `a_second_automatic_check_waits_a_full_day`, `a_last_check_in_the_future_means_the_clock_was_corrected`, `the_wait_until_the_next_check_counts_down`.
- `apps/desktop/src-tauri/tests/update_service.rs` : `the_first_launch_checks_then_the_next_automatic_check_waits_a_day`, `a_restart_within_the_day_does_not_check_again`, `a_failed_attempt_counts_it_is_not_retried_before_the_next_day`, `the_attempt_is_written_before_the_network_call`, `check_now_ignores_the_daily_limit_and_restarts_the_window`.
- `apps/desktop/src-tauri/tests/update_feed.rs` : `end_to_end_check_then_click_installs_only_a_verified_file` (une vérification = une requête, rien d'autre).

## Cas limites
- Une tentative qui échoue (pas de réseau au démarrage de Windows, par exemple) COMPTE : la suivante a lieu 24 h plus tard (client resté ouvert dans la zone de notification) ou à un « Vérifier maintenant ». Choix de la lecture stricte de « jamais plus souvent » (ADR-0017).
- Horloge remise en arrière : une dernière tentative « dans le futur » est tenue pour fausse, la vérification est permise.
- Le battement horaire ne fait aucun appel réseau tant que 24 h ne sont pas écoulées.

## Règles liées
- BR-UPDATE-006, BR-UPDATE-007, BR-UPDATE-008, BR-UPDATE-026, ADR-0017

## Historique
- 2026-10-05 : création (HRT-16, session 2026-10-04-hearth-creation).
