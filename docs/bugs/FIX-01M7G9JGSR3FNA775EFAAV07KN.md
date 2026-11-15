---
id: FIX-01M7G9JGSR3FNA775EFAAV07KN
titre: La CI devient rouge à chaque bump de version du dépôt (tests et scénario e2e qui écrivent la version courante en dur)
date_découverte: 2026-11-15
date_correction: 2026-11-15
---

# FIX-01M7G9JGSR3FNA775EFAAV07KN : Version courante du dépôt écrite en dur dans un test et dans le scénario e2e d'installation

Sévérité : majeur (main rouge depuis le bump 0.2.0 de #146, mergé sans CI).

## Symptôme
Après le passage du dépôt à 0.2.0 : `tests/update_stream.rs` (deux tests, Linux et Windows) échoue, et le job `agent-install` (`cargo xtask e2e-install`, scénario « mise à niveau 0.0.9 vers ... ») échoue sur « docker exec a échoué ».

## Cause root
`update_stream.rs` demande une mise à jour vers `0.2.0` à un agent réel dont la version est `CARGO_PKG_VERSION` : devenu 0.2.0, la cible n'est plus plus récente, refus. `deploy/e2e/scenario.sh` fabriquait l'« ancien » binaire par `sed 's/0\.1\.0/0.0.9/'` et vérifiait « 0.1.0 » après la mise à niveau : le binaire annonçant 0.2.0, le remplacement ne change rien et les vérifications échouent.

## Reproduction
Passer la version du workspace à 0.2.0 puis lancer `cargo test -p hearth-agent --test update_stream` ou `cargo xtask e2e-install`.

## Impacté
`crates/hearth-agent/tests/update_stream.rs`, `tests/support/update.rs` (`Rig::request_for`), `deploy/e2e/scenario.sh`. Les autres tests de mise à jour et `scenario-update.sh` fixent leurs versions eux-mêmes (`HEARTH_AGENT_VERSION` ou faux agent) et ne dépendent pas du dépôt.

## Workaround
Aucun.

## Correction
`update_stream.rs` vise `99.0.0` (toujours plus récente), signée pour cette version (`request_for`). `scenario.sh` lit la version courante dans `--version` du binaire et l'utilise partout (remplacement par 0.0.9 sur 5 caractères, garde explicite). Alternative écartée : calculer « version courante + 1 », inutile avec une constante haute.

## Références
- #146 (bump 0.2.0), #147
