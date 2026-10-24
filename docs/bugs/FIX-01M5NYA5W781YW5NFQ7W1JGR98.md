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
Trois chemins publiaient l'issue aux abonnés (`ServiceOps::finish`, `ServiceOps::annotate`, qui persistent puis publient) AVANT d'écrire la ligne du journal d'activité : `Deployments::settle` (BR-DEPLOY-025), `ServiceLifecycle::settle` et la surveillance de `ServiceLifecycle::watch` (BR-LIFE-018). Un client qui voyait l'issue pouvait relire un journal incomplet. Au passage, `Deployments::settle` écrivait la ligne « Succeeded » même quand `finish` refusait la transition (annulation arrivée entre `is_cancelling` et `finish`).

## Impacté
Tout lecteur du journal d'activité juste après la fin d'un déploiement ou d'une action de cycle de vie, ou après la cause posée par la surveillance (client, tests).

## Workaround
Aucun.

## Correction
`ServiceOps` sépare la persistance de la publication : `finish_quietly` et `annotate_quietly` persistent (et rendent les verrous) sans publier, `announce` publie. Les trois chemins font : persister, écrire la ligne du journal seulement si la transition est acceptée, puis `announce`. `finish` et `annotate` gardent leur comportement (persister puis publier). Le test de la surveillance n'attend plus rien : la cause n'est visible qu'après la ligne.

## Références
- Ticket : HRT-101 ; PR #116
- Règles : BR-DEPLOY-025, BR-LIFE-018
- Hors périmètre : le chemin panique de `Deployments` clôt en Failed sans ligne de journal (suivi HRT-205)
