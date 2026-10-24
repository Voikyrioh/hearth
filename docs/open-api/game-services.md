# Un serveur de jeu au quotidien (HRT-103)

Capacité `game-services` annoncée par [`GET /hello`](./hello.md). Deux choses, côté agent : l'arrêt d'un serveur de jeu qui enregistre son monde (aucune route de plus : les trois actes `start`, `stop` et `restart` de [services](./services.md) changent de déroulé pour un service créé depuis une fiche), et la révélation du mot de passe du serveur. Règles : `BR-JEU-001`, `002`, `003`, `005` à `009`, `011`, `016`, `025` à `027`, `032`, `034`, `038`, `042`, décision ADR-0052.

## Arrêter ou redémarrer un serveur de jeu

`POST /api/v1/services/{key}/stop` et `/restart` (actes `0x0D` et `0x0E`, inchangés : droit « gérer les services », confirmation, `202 { action_id, grace_s }`). Pour un service **créé par Hearth depuis une fiche** que cet agent connaît, l'exécution suit ce que la fiche déclare de l'arrêt (`stop_plan`, BR-JEU-002) :

| La fiche dit | L'agent fait |
|---|---|
| `by_image_on_signal` (7 Days to Die) | l'arrêt du moteur avec le délai du service, aucun ordre (BR-JEU-008) ; mention « non nécessaire (le jeu enregistre lui-même) » |
| `by_sequence`, suite `tested` (Terraria une fois essayée) | la suite : ordres littéraux écrits sur l'entrée du service (étape `save_world`), attente du signe ou d'un délai, ordre de sortie, puis AUSSITÔT l'arrêt demandé au moteur avec le délai du service (étape `stop`) ; pour un redémarrage, ensuite le démarrage (étape `start`) |
| `by_sequence`, suite `untested`, ou `not_saved`, ou fiche inconnue de cette construction | **rien n'est envoyé ni arrêté** : l'opération attend une décision (BR-JEU-038, BR-JEU-007) |
| (service adopté, ou lancé par une unité du système) | aucun ordre, l'arrêt de toujours (BR-JEU-001) |

- **Plan de l'opération** (`OperationView.steps`) : `save_world`, `stop` (et `start` pour un redémarrage) ; un arrêt ordinaire garde son étape unique. Aucune étape n'est destructive : une opération interrompue par un redémarrage de l'agent est `interrupted`, jamais reprise ni rejouée (BR-JEU-011).
- **Décision** : quand la confirmation manque (ordre refusé, entrée du service non ouverte, signe obligatoire non vu, suite non essayée), l'opération passe à `awaiting_decision` avec les choix `continue` (« Arrêter quand même ») et `cancel` (« Annuler »), que `POST /operations/services/{id}/decision` tranche ([service-ops](./service-ops.md)). `cancel` laisse le service en marche, l'opération passe à `cancelled` et le verrou est rendu ; sans réponse en 15 minutes elle passe à `cancelled` avec la cause `decision_expired`, le service reste en marche (BR-JEU-006). `continue` demande l'arrêt au moteur ; si seul le signe manquait, l'ordre de sortie de la fiche est envoyé d'abord. L'agent ne renvoie jamais un ordre tout seul.
- **Annulation** : `DELETE /operations/services/{id}` entre deux étapes de la suite arrête la suite avant l'étape suivante (`cancelled`, `cancelled_by_account`) ; après l'ordre de sortie, l'arrêt du moteur suit aussitôt et rien ne se défait.
- **Texte envoyé au jeu** : toujours un littéral de la fiche (64 caractères ASCII imprimables au plus), jamais un texte du client ; aucune route ne porte `ContainerEngine::send_stdin_line` (BR-JEU-016).
- **Journal d'activité** : l'entrée `service.stop` ou `service.restart` ajoute à sa cible la mention de l'enregistrement (BR-JEU-032) : « (monde enregistré) », « (enregistrement non confirmé) », « (enregistrement impossible) », « (non nécessaire (le jeu enregistre lui-même)) », « (arrêt malgré tout) », avec « , arrêt forcé » s'il l'a été. Jamais le texte d'un ordre. Le choix du compte est inscrit par la route de décision.
- **Code** : `application/game_stop.rs` (`GameStop::{lookup, run}`), `application/service_lifecycle.rs::ServiceLifecycle::run_game`, `domain/ops/game_stop.rs`.

## `GET /api/v1/services/{key}/game-secret`

Le mot de passe du serveur de jeu, pour les comptes qui ont le droit « gérer les services » (niveau `Services`, quel que soit le rôle, BR-GRANT-004). Pas un acte d'administration : aucun mot de passe demandé, comme la révélation des variables (`/env`).

- **Réponse `200`** (`GameSecret`, `Cache-Control: no-store`) : `{ "password": "…" }` ; `password` est absent quand le serveur n'en a pas (BR-JEU-027). Lu dans la description chiffrée du service (la valeur la plus récente), jamais dans le moteur ni dans le fichier du jeu (BR-JEU-025). Jamais un secret généré par Hearth ni imposé par la fiche.
- **Erreurs** : `403` sans le droit (refus consigné, `service.game.password.reveal`) ; `404 SERVICE_NOT_FOUND` pour un service inconnu, adopté ou sans fiche ; `503 BUSY` si la révélation n'a pas pu être consignée (aucune valeur n'est rendue) ; `500` si les secrets des services sont illisibles.
- **Journal d'activité** : `service.game.password.reveal` (« Mot de passe de serveur de jeu révélé »), cible « service {nom} », écrite AVANT de rendre la valeur, jamais la valeur (BR-JEU-026).
- **Code** : `entrypoint/http/game_settings.rs::game_secret`, `application/game_settings.rs::GameSettings::reveal_game_password`. Type : `hearth_proto::api::game_secret::GameSecret`.
