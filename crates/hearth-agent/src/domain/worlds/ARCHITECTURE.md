# Architecture : domain/worlds

Reprendre un monde existant (HRT-105, ADR-0059). Aucune E/S : ce dossier juge des NOMS.

| Fichier | Rôle |
|---|---|
| `mod.rs` | `WorldRules` (ce qu'est un monde pour une fiche : fichier avec suffixe et compagnons, ou dossier à une profondeur ; réglage du nom ; réglages portés), `Shape`, `looks_like_backup` (nom de sauvegarde, jamais copié ni source), bornes (`MAX_DEPTH`, `MAX_ENTRIES`, `MEASURE_BUDGET`, `COPY_MAX`, `MAX_LISTED`, `MAX_WORLD_ITEMS`) |

Règles : `BR-MONDE-003`, `005`, `006`, `011`, `014`, `039`. Utilisé par `application/world_import/` (recherche, plan, copie).
