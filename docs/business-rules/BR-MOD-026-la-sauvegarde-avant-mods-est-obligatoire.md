---
id: BR-MOD-026
domaine: MOD
titre: La sauvegarde « Avant mods » est obligatoire, sans case pour la sauter
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-026 : La sauvegarde « Avant mods » est obligatoire

## Règle
Chaque lot commence par une sauvegarde du serveur (le cœur commun de `Backups`, `copy_and_verify`), de description fixe « Avant mods, {date} » (note scellée), sans case pour la sauter. Espace insuffisant (sauvegarde plus taille des dépôts plus réserve, BR-BACKUP-009) : `BACKUP_SPACE`, rien n'est arrêté ni modifié. Une sauvegarde qui échoue (ou une vérification qui échoue) arrête le lot avant `place` : les mods n'ont pas changé, le service qui tournait est relancé. L'annulation est permise jusqu'au début de `place` (le point de non-retour) : mods inchangés, service relancé, la sauvegarde reste si elle était finie ; ensuite `TOO_LATE_TO_CANCEL`. La sauvegarde suit les règles ordinaires (jamais supprimée automatiquement).

## Application (code)
- Client (HRT-263) : `apps/desktop/src/services/modsApply.ts::spaceMissingText` (l'espace qui manque est dit avant), `ModsProgress.vue` (annulation jusqu'au début de la mise en place)
- `crates/hearth-agent/src/application/backups.rs::{Backups::prepare_for_mods, Backups::accept_for_mods, Backups::copy_and_verify}`
- `crates/hearth-agent/src/domain/ops/mods.rs::op_plan` (`place` : seule étape destructive, point de non-retour)

## Vérification
- `tests/mods_apply_https.rs` (sauvegarde citée par la réponse, annulation avant `place`)

## Règles liées
- BR-MOD-025, BR-BACKUP-009, BR-BACKUP-010, BR-BACKUP-015

## Historique
- 2026-11-18 : création (HRT-258).
