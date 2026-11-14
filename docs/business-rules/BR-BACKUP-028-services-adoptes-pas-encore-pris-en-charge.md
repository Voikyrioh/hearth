---
id: BR-BACKUP-028
domaine: BACKUP
titre: Les sauvegardes d'un service adopté ne sont pas prises en charge dans cette tranche : refus `SERVICE_ADOPTED`
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-028), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-028 : Les sauvegardes d'un service adopté ne sont pas prises en charge dans cette tranche : refus `SERVICE_ADOPTED`

## Règle
Elles ne porteraient que sur ses dossiers situés sous un dossier ouvert aux services (BR-ADOPT-028), qui n'existent pas encore côté agent (HRT-70, N6). Tant que c'est le cas, toute route de sauvegarde sur un service qui n'est pas créé par Hearth répond `409 SERVICE_ADOPTED` (journalisé pour les actes). Les dossiers du serveur d'un service créé par Hearth sont listés « Pas sauvegardé » et jamais lus.

## Application (code)
- `crates/hearth-agent/src/application/backups.rs::Backups::subject`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::an_unsafe_root_or_an_unknown_service_is_refused_without_a_trace_of_writing`

## Cas limites
- Statut `partielle` : le test couvre le service inconnu ; le refus d'un adopté suit la même branche.

## Règles liées
- BR-BACKUP-001
- BR-BACKUP-002

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
