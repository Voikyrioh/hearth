---
id: FIX-01M4D6KNHSZ39EY28XEFAV722X
titre: L'effacement de reprise des anciennes empreintes était retenté à chaque ouverture de la base
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D6KNHSZ39EY28XEFAV722X : L'effacement de reprise des anciennes empreintes était retenté à chaque ouverture de la base

## Symptôme
`VACUUM` et le point de contrôle étaient lancés à CHAQUE ouverture de la base tant que la marque n'était pas posée, sous-commandes de la ligne de commande comprises (`account revoke`, `attack-mode off`…).

## Reproduction
`infrastructure::sqlite::tests::only_the_service_start_retries_the_erasure_not_a_command_line_open`.

## Cause root
`Database::open` faisait tout, pour tous les appelants.

## Impacté
Agent et client depuis la tranche concernée (jamais publié).

## Workaround
Aucun.

## Correction
`Database::open` ne reprend plus l'effacement ; `Database::open_for_service` (démarrage du service, `app::start`) le fait et rend un `ServiceDatabase`, type que SEUL le câblage du service (`app::start_*`) accepte : il ne peut plus repasser par `open`. `ServiceDatabase::adopt` (bancs d'essai) n'est jamais appelé par le code de production : `tests/service_database_guard.rs`. `FIX:` dans `sqlite/mod.rs`.

## Règles
- BR-RESIL-021 et ADR-0034 (effacement physique des anciennes empreintes, repris au démarrage du service).

## Non-régression
- Le test ci-dessus ; `tests/migration_0008.rs` ouvre par `open_for_service`.

## Références
- Ticket : HRT-18 (suites des reviews des PR #42 et #48)
