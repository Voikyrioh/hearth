---
id: BR-SLOG-004
domaine: SLOG
titre: Pause fige l'écran, compte exactement les lignes arrivées, la reprise les insère dans l'ordre
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-004, BR-SLOG-006), HRT-67
maj: 2026-10-14
---

# BR-SLOG-004 : pause et reprise

## Règle
« Pause » fige l'affichage : aucune ligne ne s'ajoute ni ne disparaît de l'écran. Le compteur des nouvelles lignes est EXACT. Le client ne garde en attente que les 5 000 lignes les plus récentes ; à la reprise, s'il en est arrivé davantage, un repère dit combien n'ont pas été gardées. « Reprendre le direct » insère les lignes manquées dans l'ordre et cale la vue en bas. Le compteur n'est pas dans une zone `aria-live` (aucune annonce par ligne).

## Application (code)
- HORS DOMAIN : `apps/desktop/src/stores/serviceLogs.ts::{pause, resume, receive}`, `apps/desktop/src/logs/buffer.ts` (tampon non réactif), `apps/desktop/src/components/organisms/LogViewer.vue` (barre, bouton flottant).

## Vérification
- `apps/desktop/src/stores/serviceLogs.test.ts` (pause, compteur, plafond en attente), `apps/desktop/e2e/hrt67-logs.spec.ts` (parcours, charge)

## Règles liées
- BR-SLOG-005, BR-SLOG-006

## Historique
- 2026-10-14 : fiche créée avec l'onglet Journaux du client (HRT-67).
