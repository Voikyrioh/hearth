---
id: FIX-01M855R2F0XJ3KDBH325R1NR2R
titre: La lecture des chemins de l'hôte refusait au-delà de huit lectures simultanées : « racine des sauvegardes indisponible » à tort
date_découverte: 2026-11-23
date_correction: 2026-11-23
---

# FIX-01M855R2F0XJ3KDBH325R1NR2R : une lecture de chemin attend une place au lieu d'être refusée

## Symptôme
Sur un agent chargé (plusieurs sauvegardes, suppressions ou lots de mods en même temps) : `backups_root_unavailable`, une sauvegarde refusée alors que le dossier est sain. Dans les tests, `service_backups_https`, `service_delete_https` et `mods_apply_https` échouaient de façon instable selon l'ordre et le nombre de fils.

## Reproduction
Huit lectures de chemin en vol (ou coincées sur un partage lent), une neuvième : `SystemHostPaths::resolve` rendait `Resolved::Unreadable` tout de suite, sans lancer de fil. Le compteur étant un `static` du processus, tous les tests d'un même binaire se disputaient les huit places.

## Cause root
`infrastructure/host/paths.rs::within` refusait immédiatement quand `MAX_READERS` (8) était atteint. Le plafond existe pour qu'un partage mort ne fasse pas naître des fils sans fin ; mais il refusait aussi quand les huit lectures étaient SAINES et allaient finir dans la milliseconde. Un refus immédiat est lu comme « on ne sait pas » (jamais un feu vert), donc comme une racine indisponible.

## Impacté
Toute fonction qui lit un chemin de l'hôte : racine des sauvegardes (`Backups::path_facts`), suppression d'un service (BR-DEL-008), racine de suppression au démarrage.

## Workaround
Aucun côté Hearth ; relancer l'opération refusée.

## Correction
- `Gate` (compteur et `Condvar`) remplace le compteur atomique : une lecture ATTEND une place, au plus 10 secondes (`WAIT_FOR_SLOT`), puis seulement répond « illisible ». Le plafond de fils reste de 8 (un partage mort ne fait pas naître de fil de plus) ; la lecture reste bornée à 3 secondes une fois lancée.
- Les appelants sont déjà dans `spawn_blocking` : l'attente ne bloque aucun fil asynchrone.
- Bancs de test : `parts_supervised` pose par défaut `FakeHostPaths` (même lecture, sans plafond partagé) ; les tests qui visent le vrai lecteur règlent `host_paths` eux-mêmes.
Non-régression : `infrastructure::host::paths::tests::a_read_waits_for_a_place_instead_of_being_refused_while_the_others_finish`, `..::stuck_readers_make_the_next_one_wait_then_give_up_unreadable_not_a_thread_more` ; `tests/service_backups_https.rs`, `tests/service_delete_https.rs`, `tests/mods_apply_https.rs` stables à 2 fils.

## Références
- Règles : BR-DEL-008, BR-BACKUP-009
- Code : `crates/hearth-agent/src/infrastructure/host/paths.rs` (marqueur `FIX:01M855R2F0XJ3KDBH325R1NR2R`), `crates/hearth-agent/tests/support/fake_host.rs::FakeHostPaths`
