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

> **Partielle** : la moitié agent (garder le poste, 90 jours, retrait, réinscription par mot de passe) est livrée par HRT-22 ; la clé côté client et son effacement à la désinstallation sont HRT-23.

## Application (code)
- Agent : `crates/hearth-agent/src/domain/trust/device.rs::RETENTION` ; `application/maintenance.rs::MaintenanceService::purge` ; `application/trust.rs::TrustService::remove`.
- Client (HRT-23, pas encore) : coffre `Hearth/{id du serveur}/device-key`.

## Vérification
- `tests/device_proof.rs::the_purge_forgets_a_device_after_ninety_days_without_proof_and_detaches_its_session`, `the_same_key_presented_twice_is_one_device_proven_the_second_time`.

## Cas limites
- BR-TRUST-026 de la spec disait « une nouvelle clé est créée » à la réinstallation : corrigée par Q14 (point 6), conformément à BR-CLIENT-008.

## Règles liées
- BR-TRUST-003, BR-TRUST-022, BR-CLIENT-008, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
