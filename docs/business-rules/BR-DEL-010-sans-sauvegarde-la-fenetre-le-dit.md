---
id: BR-DEL-010
domaine: DEL
titre: Sans sauvegarde que Hearth connaisse, la fenêtre le dit et n'en force aucune
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-010), HRT-86
maj: 2026-11-14
---

# BR-DEL-010 : Sans sauvegarde que Hearth connaisse, la fenêtre le dit et n'en force aucune

## Règle
Quand la case « Supprimer aussi les données » est cochée, la fenêtre dit la dernière sauvegarde que Hearth a faite de ce service. Tant que Hearth n'en fait pas (HRT-97), elle écrit « Vous n'avez aucune sauvegarde de ce service dans Hearth. » sans lien « Sauvegarder d'abord » (l'onglet Sauvegardes n'existe pas encore). Hearth ne force jamais de sauvegarde et ne mentionne pas celles qui ont été faites hors de Hearth.

## Application (code)
- `apps/desktop/src/components/organisms/DeleteServiceDialog.vue` (`data-delete-no-backup`)
- `apps/desktop/src/i18n/fr.ts` (`serviceDelete.noBackup`)

## Vérification
- `apps/desktop/src/pages/ServicesDelete.test.ts`

## Cas limites
- Le lien « Sauvegarder d'abord » et la date de la dernière sauvegarde arrivent avec les sauvegardes (HRT-97, `DeletePreview::backups`).

## Règles liées
- BR-DEL-005
- BR-DEL-011

## Historique
- 2026-11-14 : création (HRT-86, côté client).
