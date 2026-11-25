---
id: FIX-01M8CJ9T4Q6W2XKZ7N3VHBY5PD
titre: Un cycle d'Arc entre les sauvegardes et le service des mods gardait la base SQLite ouverte après l'arrêt de l'agent
date_découverte: 2026-11-24
date_correction: 2026-11-24
---

# FIX-01M8CJ9T4Q6W2XKZ7N3VHBY5PD : le port des mods des sauvegardes est pris en `Weak`

## Symptôme
CI Windows : `cargo xtask test-tmp-check` échoue, 240 dossiers de test laissés par `hearth-agent` et 87 par `hearth-desktop`. Chaque test affiche « dossier de test non supprimé … (os error 32) » : `hearth.db`, `-wal` et `-shm` restent ouverts après `shutdown()`.

## Reproduction
`cargo test -p hearth-agent --test hello` sous Windows : 5 dossiers laissés sur 6 tests, avant la correction ; 0 après.

## Cause root
`app.rs` branchait `backups.set_mods_snapshot(mods)` (HRT-293) : `Backups` tenait un `Arc<ModsService>`. Or `mods.set_apply(ApplyDeps { backups, .. })` (HRT-258) fait tenir à `ModsService` un `Arc<Backups>`. Deux `Arc` qui se tiennent l'un l'autre : ni l'un ni l'autre ne tombe, le pool SQLite (cloné dans les deux) reste vivant après l'arrêt, Windows refuse de supprimer le dossier.

## Impacté
Tout agent démarré puis arrêté dans un même processus : les tests d'intégration (fuite de fichiers, dossiers temporaires), pas la production (le processus meurt).

## Workaround
Aucun.

## Correction
- `Backups` prend le port des mods en `Weak` (`set_mods_snapshot`) ; `keep_mods_snapshot` garde un faux des essais (qui ne tient pas les sauvegardes) par un `Arc`.
- `app.rs` : le vrai service est pris en `Weak`, le faux est gardé.
Non-régression : `cargo xtask test-tmp-check --expect-tests` (0 dossier laissé) ; `tests/hello.rs`.

## Références
- Code : `crates/hearth-agent/src/application/backups.rs` et `src/app.rs` (marqueur `FIX:01M8CJ9T4Q6W2XKZ7N3VHBY5PD`)
