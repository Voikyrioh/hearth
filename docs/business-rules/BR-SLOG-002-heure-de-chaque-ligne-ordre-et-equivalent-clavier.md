---
id: BR-SLOG-002
domaine: SLOG
titre: Chaque ligne montre son heure dans le fuseau du PC ; ordre du moteur, jamais de doublon, équivalent clavier de l'info-bulle
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-002), HRT-67
maj: 2026-10-14
---

# BR-SLOG-002 : heure de chaque ligne

## Règle
Chaque ligne montre son heure `HH:MM:SS` dans le fuseau du PC ; la date et les millisecondes se lisent au survol (info-bulle). Les lignes sont dans l'ordre où le moteur les a reçues, la plus récente en bas, aucune n'est affichée deux fois (une reprise après coupure ne rejoue rien ; une reprise perdue retire de l'écran les lignes que la page suivante répète). Au clavier, la ligne active (flèches haut et bas) donne sa date et son heure complètes dans la ligne d'information en bas de la fenêtre. Une heure illisible s'écrit `--:--:--` : elle n'est jamais inventée.

## Application (code)
- HORS DOMAIN : règle portée par l'interface. `apps/desktop/src/logs/time.ts::{clockOf, fullTimeOf}`, `apps/desktop/src/logs/buffer.ts::LogBuffer::dropOverlap`, `apps/desktop/src/components/organisms/LogViewer.vue` (ligne d'information, `title` de l'heure).

## Vérification
- `apps/desktop/src/logs/logs.test.ts` (heures, recouvrement), `apps/desktop/src/stores/serviceLogs.test.ts` (reprise sans doublon), `apps/desktop/e2e/hrt67-logs.spec.ts`

## Règles liées
- BR-SLOG-014

## Historique
- 2026-10-14 : fiche créée avec l'onglet Journaux du client (HRT-67).
