---
id: BR-MONDE-013
domaine: MONDE
titre: Une reprise de monde partage la place `heavy_io` avec les sauvegardes et restaurations ; on peut l'annuler pendant la copie
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-013), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-15
---

# BR-MONDE-013 — Une reprise de monde partage la place `heavy_io` avec les sauvegardes et restaurations ; on peut l'annuler pendant la copie

## Règle
L'installation avec un monde prend la place des opérations lourdes du serveur à son acceptation : une seconde est refusée (`HEAVY_IO_BUSY`), jamais mise en file. Elle compte dans la limite de deux déploiements. L'annulation arrête la copie et retire seulement le dossier d'import créé par l'opération, identifié par son inode ; la source n'est pas touchée.

## Application (code)
- `crates/hearth-agent/src/application/deployments.rs::Deployments::{deploy, import_world, discard_import}`
- `crates/hearth-agent/src/domain/ops/locks.rs::LockTable::acquire_heavy_io`
- `apps/desktop/src/components/organisms/DeployProgressDialog.vue` (étape « Copie du monde », libellé « Annuler la reprise » pendant la copie) et `apps/desktop/src/link/deploy.ts::DEPLOY_STEPS_WITH_WORLD`

## Vérification
- `crates/hearth-agent/tests/world_import_https.rs::one_heavy_operation_at_a_time_and_the_second_world_is_refused_not_queued`
- `crates/hearth-agent/tests/world_import_https.rs::cancelling_during_the_copy_removes_the_import_and_creates_nothing`
- `crates/hearth-agent/src/infrastructure/world_fs/unix.rs::tests::only_the_import_directory_of_the_operation_is_removed`
- `apps/desktop/src/pages/InstallWorld.test.ts` (copie, annulation, reprise déjà en cours)

## Cas limites
- L'annulation est prise en compte à la seconde près pendant la copie.

## Règles liées
- place des opérations lourdes (HRT-97), BR-MONDE-026

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
- 2026-11-15 — côté client (HRT-106).
