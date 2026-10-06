---
id: FIX-01M46N01GMK08NXQHCZ28A2KQ1
titre: Une désinstallation lancée juste après une installation pouvait être refusée à tort (« installation déjà en cours »)
date_découverte: 2026-10-05
date_correction: 2026-10-05
---

# FIX-01M46N01GMK08NXQHCZ28A2KQ1 : Verrou d'installation relâché trop tard

## Symptôme
CI du 2026-10-05, job `rust (ubuntu-latest)`, run 37351903191, 1re tentative : `a_purge_of_a_directory_that_holds_only_hearth_files_removes_the_directory` échoue à `uninstall(true).unwrap()` avec `Refused("Une installation est déjà en cours sur ce serveur...")`. Vert à la 2e tentative.

## Reproduction
Linux (image `rust:1.95`), binaire de `tests/install_fs.rs` lancé en boucle avec 6 boucles CPU en parallèle : 5 échecs sur 100 (tests purge et désinstallation, jamais le même). Avec `--test-threads=1` : 0 sur 200. Après correctif : 0 sur 300 (parallèle, root), 0 sur 100 (parallèle, non root), 0 sur 50 (`--test-threads=1`).

## Cause root
Un `flock` appartient à la **description de fichier ouverte**, pas au descripteur. `SystemHost::lock` (`crates/hearth-agent/src/infrastructure/install/host.rs`) ne relâchait le verrou qu'en fermant son descripteur. Dès qu'un autre thread du même processus lance un sous-processus (`scrubbed(...).spawn()` : `--version` du binaire installé, `systemctl`, `df`), l'enfant porte, entre le `fork` et l'`exec` (qui le ferme grâce à `O_CLOEXEC`), une copie de tous les descripteurs ouverts, verrou compris. Fermer le nôtre ne libère alors rien tant que l'enfant n'a pas fait son `exec`. Sous charge, la fenêtre dure assez pour que l'opération suivante (`uninstall` juste après `install`) trouve le verrou tenu.
Hypothèse « tâche bloquante ou valeur gardée par une tâche détachée » : réfutée (le verrou est une variable locale `_lock` de `install`/`uninstall`, détruite au retour ; `install` ne lance aucune tâche détachée).

## Impacté
Défaut surtout de test (plusieurs tests parallèles dans un même processus, chacun lançant des enfants). Latent côté produit : le CLI `install`/`uninstall` est un flux unique ; un refus à tort demanderait un lancement de sous-processus par un autre thread au moment exact du relâchement.

## Workaround
Relancer.

## Correction
`HeldLock` (même fichier) : `Drop` appelle `File::unlock()` (`flock(LOCK_UN)`), qui libère le verrou pour **toutes** les copies de la description. Le verrou n'est pas affaibli : toujours exclusif, pris par `try_lock`, relâché à la destruction. Rejeté : attente, relance, ouverture du verrou en `O_CLOEXEC` seul (déjà le cas, ne couvre pas la fenêtre fork/exec).
Non-régression : `infrastructure::install::host::unix_tests::the_install_lock_is_released_while_a_copy_of_its_descriptor_is_still_alive` (copie du descripteur = enfant forké ; rouge avant, vert après, déterministe).
Suite (2026-10-06, HRT-17, tâche T27) : `HeldLock` est devenu une brique partagée (`crates/hearth-agent/src/infrastructure/file_lock.rs`, `HeldLock::acquire`) et le même relâchement explicite s'applique au verrou du superviseur (`infrastructure/update/host.rs`) et au verrou de création de l'identité TLS (`infrastructure/tls/identity.rs`). Exclusivité entre processus inchangée. Non-régression, une par verrou (copie du descripteur vivante, rouge sans le `unlock`, vérifié sous Linux) : `file_lock::tests::the_lock_is_released_while_a_copy_of_its_descriptor_is_still_alive`, `infrastructure::update::host::tests::the_supervisor_lock_is_released_while_a_copy_of_its_descriptor_is_still_alive`, `infrastructure::tls::identity::tests::the_creation_lock_is_released_while_a_copy_of_its_descriptor_is_still_alive`. Le test de l'installation passe par `SystemHost::lock`.

## Références
- Ticket : HRT-15 (installation), suivi de HRT-17
- Règle : « une seule installation à la fois » (spec `contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-agent.md`, cas limites ; pas de fiche BR dédiée) : conforme, inchangée
- Code : `crates/hearth-agent/src/infrastructure/install/host.rs` (marqueur `FIX:01M46N01GMK08NXQHCZ28A2KQ1`)
