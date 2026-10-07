---
id: BR-TRUST-026
domaine: TRUST
titre: Une réinstallation ou une mise à jour du client garde la clé de l'appareil ; seule une désinstallation complète ou un nouveau PC en crée une nouvelle
statut: partielle
invariant: false
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md ; conception technique 2026-10-06 ; contexts/hearth/tickets/hrt/HRT-22.md ; ADR-0023
maj: 2026-10-07
---

# BR-TRUST-026 : Une réinstallation ou une mise à jour du client garde la clé de l'appareil ; seule une désinstallation complète ou un nouveau PC en crée une nouvelle

## Règle
Une réinstallation ou une mise à jour du client Hearth garde la clé de l'appareil (BR-CLIENT-008 : la réinstallation garde les données ; la clé vit au coffre de Windows, avec le carnet). Seule une désinstallation complète avec suppression des données, ou un nouveau PC, crée une nouvelle clé à la prochaine connexion réussie par mot de passe (hors mode attaque). L'**ancien poste reste dans la liste** jusqu'à son retrait par le titulaire ou son oubli par le serveur (90 jours sans preuve).

> **Partielle** : le client garde la clé (coffre, carnet) à la mise à jour et à la réinstallation qui gardent les données (HRT-23). Reste à faire : l'effacement des entrées `Hearth/*` à la désinstallation complète (« Tout effacer »), suivi de l'ADR-0013.

## Application (code)
- Agent : `crates/hearth-agent/src/domain/trust/device.rs::RETENTION` ; `application/maintenance.rs::MaintenanceService::purge` ; `application/trust.rs::TrustService::remove`.
- Client : coffre `Hearth/{id du serveur}/device-key` (`apps/desktop/src-tauri/src/vault.rs::credential_target`) ; `crates/hearth-link/src/manager/mod.rs::remove_server` l'efface avec le serveur.

## Vérification
- `tests/device_proof.rs::the_purge_forgets_a_device_after_ninety_days_without_proof_and_detaches_its_session`, `the_same_key_presented_twice_is_one_device_proven_the_second_time`.
- Client : `crates/hearth-link/tests/device_key.rs::restarting_the_client_keeps_the_key_and_the_enrolled_pc` (même clé, aucun nouveau poste), `::one_key_per_server_and_removing_a_server_removes_its_key`.

## Cas limites
- BR-TRUST-026 de la spec disait « une nouvelle clé est créée » à la réinstallation : corrigée par Q14 (point 6), conformément à BR-CLIENT-008.

## Règles liées
- BR-TRUST-003, BR-TRUST-022, BR-CLIENT-008, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
