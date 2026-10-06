---
id: BR-TRUST-022
domaine: TRUST
titre: Un compte a jusqu'à 8 postes inscrits, le 9e n'est pas inscrit sans rien supprimer ; le titulaire voit la liste et en retire un
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md ; conception technique 2026-10-06 ; contexts/hearth/tickets/hrt/HRT-22.md ; ADR-0023
maj: 2026-10-07
---

# BR-TRUST-022 : Un compte a jusqu'à 8 postes inscrits, le 9e n'est pas inscrit sans rien supprimer ; le titulaire voit la liste et en retire un

## Règle
Un compte peut avoir jusqu'à **8** postes inscrits. Un poste n'est inscrit que par une connexion réussie avec le mot de passe, jamais par une session seule (BR-TRUST-004). Quand le compte a déjà 8 postes, le 9e n'est **pas** inscrit : la connexion réussit quand même (`device: "limit"`), **sans suppression automatique** d'un poste existant. Le titulaire voit la liste de ses postes (`GET /me/devices`) et peut en retirer un (`DELETE /me/devices/{id}`) ; le poste retiré peut ensuite être inscrit de nouveau par une connexion par mot de passe.

- La liste ne montre que les postes de l'appelant : nom, dates, dernière adresse, et lequel est « courant » ; jamais une clé, une empreinte de clé, un défi.
- Retirer un poste retire aussi **son adresse retenue et ses sessions** (accès révoqué) : un portable perdu ne garde pas une session vivante. Le poste d'où part la requête ne se retire pas depuis lui-même (`422`). Un poste d'un autre compte, ou qui n'existe pas : `404` (indiscernables).
- Journal : `device.remove`, réussi, avec le nom du poste retiré (nettoyé).
- Un poste est oublié 90 jours après sa dernière preuve (purge horaire).

## Application (code)
- `crates/hearth-agent/src/domain/trust/device.rs::{judge_enrollment, RETENTION, cutoff}` ; `hearth-proto/src/api/devices.rs::MAX_DEVICES_PER_ACCOUNT`.
- `crates/hearth-agent/src/application/trust.rs::TrustService::{list, remove}` ; `entrypoint/http/devices.rs::{list, remove}` ; `application/maintenance.rs::MaintenanceService::purge`.

## Vérification
- `tests/device_proof.rs` : `the_ninth_device_is_refused_without_evicting_another_and_the_connection_still_succeeds`, `the_list_shows_only_the_own_devices_and_marks_the_current_one`, `the_current_device_cannot_be_removed_from_itself_but_another_can`, `removing_a_device_closes_its_sessions_forgets_its_address_and_is_journaled`, `a_device_identifier_that_is_not_ours_cannot_be_removed`, `the_purge_forgets_a_device_after_ninety_days_without_proof_and_detaches_its_session`.
- `tests/device_http.rs` : `the_list_shows_the_devices_of_the_caller_without_any_key_material`, `a_device_is_removed_by_its_owner_only_and_never_from_itself`, `removing_a_device_replays_with_its_operation_key_and_leaves_one_journal_entry`.
- `tests/http_api.rs::every_modifying_route_leaves_exactly_one_success_entry` (le retrait laisse une seule entrée réussie).

## Cas limites
- « Reconnu manuellement sur la liste » (texte de la spec) n'a pas d'autre sens exploitable que « retiré puis réinscrit » : pas d'éviction automatique (conception, section 18, à confirmer).
- Un poste purgé (90 jours) laisse ses sessions ouvertes, détachées du poste.

## Règles liées
- BR-TRUST-004, BR-TRUST-024, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
