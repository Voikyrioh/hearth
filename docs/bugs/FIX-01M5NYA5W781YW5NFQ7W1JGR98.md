---
id: FIX-01M5NYA5W781YW5NFQ7W1JGR98
titre: Une opération de déploiement annoncée terminée avant l'écriture de son entrée au journal d'activité (HRT-101)
date_découverte: 2026-10-24
date_correction: 2026-10-24
---

# FIX-01M5NYA5W781YW5NFQ7W1JGR98 : Une opération de déploiement annoncée terminée avant l'écriture de son entrée au journal d'activité (HRT-101)

## Symptôme
Test instable en CI Linux : `fiche_install_https::a_fiche_install_creates_a_service_with_the_fiche_digest_the_pinned_image_and_a_0600_root_config_file` lisait un journal vide (`left: 0, right: 1`) juste après avoir vu l'opération `Succeeded`. Même famille, une fois : `service_lifecycle_https::a_service_that_stops_right_after_its_start_is_noted_on_the_finished_action`.

## Reproduction
Course, rare : lire `GET /operations/services/{id}` jusqu'à un statut final, puis le journal d'activité (`service.deploy`) immédiatement.

## Cause root
`DeploymentService::settle` clôturait l'opération (`ops.finish`, l'état visible par le client) puis écrivait l'entrée du journal (`record`). Un lecteur qui voyait l'état final pouvait relire un journal encore incomplet. Pour le cycle de vie, la surveillance note la cause (`annotate`) puis écrit au journal : l'écriture suit forcément la note, le test ne l'attendait pas.

## Impacté
Tout lecteur du journal d'activité juste après la fin d'un déploiement (client, tests).

## Workaround
Aucun.

## Correction
Déploiement : `record` est appelé avant `ops.finish` dans `settle` (`application/deployments.rs`). Cycle de vie : la note de surveillance est par nature asynchrone (la cause dépend du résultat de `annotate`), le test attend donc la ligne du journal avec une échéance de 5 s.

## Références
- Ticket : HRT-101
