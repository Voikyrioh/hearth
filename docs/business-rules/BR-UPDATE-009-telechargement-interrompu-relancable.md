---
id: BR-UPDATE-009
domaine: UPDATE
titre: Un téléchargement interrompu peut être relancé, la version en cours reste utilisable
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-009), HRT-16
maj: 2026-10-05
---

# BR-UPDATE-009 : Un téléchargement interrompu peut être relancé, la version en cours reste utilisable

## Règle
Une coupure pendant le téléchargement (ou une réponse incomplète, ou un serveur qui répond en erreur) abandonne la mise à jour : le bandeau dit « Téléchargement interrompu. La version en cours reste utilisable. » avec « Réessayer » (qui relance tout le téléchargement) et « Plus tard ». Rien n'est écrit sur le disque (le téléchargement est en mémoire) : il n'y a aucun fichier partiel à nettoyer, et l'application n'a jamais cessé d'être utilisable.

## Application (code)
- `apps/desktop/src-tauri/src/update/feed.rs::classify` (erreur réseau du greffon : `Interrupted`).
- `apps/desktop/src-tauri/src/update/service.rs::UpdateService::{run_install, settle}`.
- `apps/desktop/src/components/organisms/UpdateBanner.vue` (message exact, « Réessayer »).

## Vérification
- `apps/desktop/src-tauri/tests/update_feed.rs` : `a_download_cut_halfway_is_interrupted_and_can_be_run_again`, `a_failing_download_server_is_interrupted_not_corrupted`, `end_to_end_a_cut_download_then_a_second_try_succeeds`.
- `apps/desktop/src-tauri/tests/update_service.rs` : `an_interrupted_download_can_be_started_again`.
- `apps/desktop/e2e/updates.spec.ts` : « téléchargement interrompu : message, version en cours utilisable, relançable ».

## Cas limites
- Pas de reprise à l'octet : on retélécharge en entier (installateur de quelques dizaines de Mo).

## Règles liées
- BR-UPDATE-002, BR-UPDATE-010

## Historique
- 2026-10-05 : création (HRT-16, session 2026-10-04-hearth-creation).
