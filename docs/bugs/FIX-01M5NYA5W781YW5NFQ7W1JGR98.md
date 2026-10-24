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
`ServiceOps::finish_with` et `ServiceOps::annotate_with` (`application/service_ops.rs`) écrivent la transition (ou la cause) ET l'entrée du journal d'activité dans UNE transaction, comme `ServiceOpAccess::cancel` : le GET ne voit l'issue qu'avec sa ligne, et une transition refusée n'écrit rien. Puis ils annoncent (flux et projection) et rendent les verrous dans cet ordre : une autre opération du service n'est acceptée qu'après l'annonce de la première. `ServiceOps::bind_audit` branche le magasin et la diffusion du journal à l'assemblage. `finish` et `annotate` restent les variantes sans entrée. Une annulation qui gagne la course sort en `info`, plus en `error`. Les champs `sink` morts de `Deployments` et `ServiceLifecycle` sont retirés. Les tests lisent l'issue par le GET puis le journal, sans attente.

## Références
- Ticket : HRT-101 ; PR #116
- Règles : BR-DEPLOY-025, BR-LIFE-018
- Hors périmètre : le chemin panique de `Deployments` clôt en Failed sans ligne de journal (suivi HRT-205)
