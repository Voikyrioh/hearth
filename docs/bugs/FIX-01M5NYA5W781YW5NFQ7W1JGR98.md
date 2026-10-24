---
id: FIX-01M5NYA5W781YW5NFQ7W1JGR98
titre: Une opération de déploiement ou de cycle de vie était annoncée terminée avant l'écriture de son entrée au journal d'activité (HRT-101)
date_découverte: 2026-10-24
date_correction: 2026-10-24
---

# FIX-01M5NYA5W781YW5NFQ7W1JGR98 : Une opération de déploiement ou de cycle de vie était annoncée terminée avant l'écriture de son entrée au journal d'activité (HRT-101)

## Symptôme
Test instable en CI Linux : `fiche_install_https::a_fiche_install_creates_a_service_with_the_fiche_digest_the_pinned_image_and_a_0600_root_config_file` lisait un journal vide (`left: 0, right: 1`) juste après avoir vu l'opération `Succeeded`. Même famille, une fois : `service_lifecycle_https::a_service_that_stops_right_after_its_start_is_noted_on_the_finished_action`.

## Reproduction
Course, rare : lire `GET /operations/services/{id}` jusqu'à un statut final (ou jusqu'à la cause « arrêté juste après le démarrage »), puis le journal d'activité immédiatement.

## Cause root
Trois chemins rendaient l'issue lisible AVANT d'écrire la ligne du journal d'activité : `Deployments::settle` (BR-DEPLOY-025), `ServiceLifecycle::settle` et la surveillance `ServiceLifecycle::watch` (BR-LIFE-018). Il y a deux lecteurs : le flux des opérations (publié par `ServiceOps::publish`) et `GET /operations/services/{id}`, qui lit le journal des opérations persisté (`ServiceOpAccess::read` puis `ops.snapshot`), pas le flux. Les deux voyaient l'issue (ou la cause) avant la ligne. Au passage, `Deployments::settle` écrivait la ligne « Succeeded » même quand la transition était refusée (annulation arrivée entre `is_cancelling` et `finish`).

## Impacté
Tout lecteur du journal d'activité juste après la fin d'un déploiement ou d'une action de cycle de vie, ou après la cause posée par la surveillance (client, tests).

## Workaround
Aucun.

## Correction
`ServiceOps::finish_with` et `ServiceOps::annotate_with` (`application/service_ops.rs`) écrivent la transition (ou la cause) ET l'entrée du journal d'activité (`AuditDraft`, un événement sans sa date) dans UNE transaction, comme `ServiceOpAccess::cancel` : le GET ne voit l'issue qu'avec sa ligne, et une transition refusée n'écrit rien. Dans l'ordre : commit, verrous rendus, annonce (flux et projection), comme `move_to` et `settle` : l'action suivante du service n'est jamais refusée une fois l'issue lisible. Si l'écriture de l'entrée échoue, l'opération est close sans elle dans une seconde transaction (`error!` sans valeur) au lieu de rester `running` avec son verrou. `finish` (sans entrée) passe par le même chemin ; `annotate` est supprimé (aucun appelant en production). Le magasin et la diffusion du journal sont passés au constructeur de `ServiceOps` (`service_foundations_observed` les reçoit de `app.rs`). L'UPDATE de `annotate_cause` est partagé entre le bassin et la transaction (`do_annotate`). Les clôtures ne sont jamais regroupées (BR-AUDIT-007). Une annulation qui gagne la course sort en `info`. Les champs `sink` morts de `Deployments` et `ServiceLifecycle` sont retirés. Tests : 20 enchaînements arrêt puis démarrage sans refus, et une clôture dont l'écriture du journal est refusée.

## Références
- Ticket : HRT-101 ; PR #116
- Règles : BR-DEPLOY-025, BR-LIFE-018, BR-AUDIT-007
- Hors périmètre : le chemin panique de `Deployments` clôt en Failed sans ligne de journal (suivi HRT-205)
