---
id: BR-UPDATE-010
domaine: UPDATE
titre: Une mise à jour corrompue ou incomplète est refusée, la version en cours reste utilisable
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-010), HRT-16
maj: 2026-10-05
---

# BR-UPDATE-010 : Une mise à jour corrompue ou incomplète est refusée, la version en cours reste utilisable

## Règle
Un fichier dont la signature ne correspond pas (modifié, tronqué, signé par une autre clé, signature absente ou illisible, format qui n'est pas un installateur) est REFUSÉ : rien n'est installé, rien n'est exécuté. Le bandeau dit « Mise à jour corrompue. Refusée. La version en cours reste utilisable. ». Le refus ne dépend pas de la forme de la corruption : un fichier complet mais plus court, un octet changé et une signature d'une autre clé aboutissent au même verdict.

## Application (code)
- `apps/desktop/src-tauri/src/update/feed.rs::classify` (`Minisign`, `Base64`, `SignatureUtf8`, `InvalidUpdaterFormat`… : `Corrupted`).
- `apps/desktop/src-tauri/src/update/service.rs::UpdateService::run_install` (jamais d'appel à `Feed::install` après un refus).

## Vérification
- `apps/desktop/src-tauri/tests/update_feed.rs` : `a_signature_from_another_key_is_refused_as_corrupted`, `a_file_altered_after_signing_is_refused_as_corrupted`, `a_complete_but_shorter_file_is_refused_as_corrupted`, `a_garbage_signature_is_refused_as_corrupted`, `an_old_installer_validly_signed_for_another_version_is_refused_when_announced_newer`, `a_signature_without_a_version_is_refused`, `end_to_end_a_tampered_installer_is_refused_with_the_corrupted_failure`.
- `apps/desktop/src-tauri/tests/update_service.rs` : `a_corrupted_update_is_refused_and_never_installed`.
- `apps/desktop/e2e/updates.spec.ts` : « mise à jour corrompue : refusée avec le message exact ».

## Cas limites
- La signature protège le fichier ET sa version : le client exige (`requireSignedVersion`) que la version annoncée par le manifeste soit celle du commentaire signé. Un ancien installateur, validement signé, servi sous un numéro plus grand est refusé comme corrompu ; une signature sans version aussi. Le manifeste n'est pas signé (HTTPS vers GitHub) : la source de l'installateur est restreinte au dépôt (BR-UPDATE-003).

## Règles liées
- BR-UPDATE-004, BR-UPDATE-009, ADR-0017

## Historique
- 2026-10-05 : création (HRT-16, session 2026-10-04-hearth-creation).
