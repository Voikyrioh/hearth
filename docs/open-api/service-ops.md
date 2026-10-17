# Opérations de service : lire, décider, annuler, suivre (HRT-144, ADR-0045)

Le journal des opérations de service (`service_ops`, `service_op_steps`) rendu par l'API. Trois routes et un message du flux. **Aucun geste de service (démarrer, déployer, supprimer…) n'existe encore** : les lots qui les écrivent (HRT-64, HRT-68…) acceptent leurs opérations dans ce journal et n'ont plus qu'à s'en servir. Code : `crates/hearth-agent/src/entrypoint/http/service_ops.rs`, `application/service_op_access.rs`, `entrypoint/service_ops_wire.rs`. Règles : `BR-OPS-001` à `BR-OPS-008`, `BR-LIFE-013`, `BR-LIFE-020`, `BR-EDIT-031`, `BR-JEU-006`.

**Capacité** : `GET /hello` annonce `service-ops`. Elle dit que ces trois routes et le message `operation` existent ; elle ne dit pas que l'agent sait démarrer ou déployer. Un client n'appelle ces routes que si elle est annoncée.

## `GET /api/v1/operations/services/{id}`

- **Authentification** : jeton. **Rôle et droit** : tout compte authentifié. Lecture seule, jamais journalisée.
- `{id}` : l'identifiant de l'opération (lettres et chiffres, 64 au plus ; un ULID). Une opération se lit pendant 24 heures après sa fin.
- **Qui voit quoi (BR-OPS-001)** : une opération n'est lisible que par qui peut voir le SERVICE qu'elle touche : service créé par Hearth (`h_`) : tous les comptes ; service adopté : le droit « gérer les services » ; tout autre : personne. Pour qui ne la voit pas, comme pour un identifiant inconnu, mal formé ou purgé : `404 OPERATION_NOT_FOUND`, **même corps**.

### Réponse `200` : `OperationView`

```json
{
  "id": "01J9ZY0G3Q8M2K6W4T7V5N1B9D",
  "key": "h_01J9ZY0G3Q8M2K6W4T7V5N1B9E",
  "kind": "deploy",
  "status": "running",
  "step": "pull",
  "steps": [{ "step": "check", "state": "done" }, { "step": "pull", "state": "intent" }],
  "percent": 40,
  "bytes_done": 400,
  "bytes_total": 1000,
  "cancellable": true,
  "decision": { "choices": ["continue", "cancel"], "expires_at": "2026-10-14T10:15:00Z" },
  "cause": "decided_by_account"
}
```

- `kind` : `start`, `stop`, `restart`, `deploy`, `update`, `delete`, `resources`, `backup`, `restore`, `world_import`, `unknown`. `status` : `accepted`, `running`, `awaiting_decision`, `cancelling`, `cancelled`, `rolling_back`, `rolled_back`, `succeeded`, `failed`, `interrupted`, `needs_attention`, `unknown`. `cause` et les choix : listes fermées avec `unknown` (un client lit toute valeur qu'il ne connaît pas comme `unknown`, jamais comme une erreur).
- `steps` : les étapes du plan qui ont une ligne au journal, dans l'ordre du plan, avec leur DERNIER état (`intent`, `done`, `failed`, `undone`). `step` : l'étape courante. Les compteurs (`percent`, `items_done`, `bytes_done`, `bytes_total`, `eta_s`) sont facultatifs.
- `cancellable` : l'annulation est encore possible (avant le point de non-retour du genre, dans un statut qui s'annule).
- `decision.expires_at` : l'échéance d'une décision demandée (15 minutes) ; **absent** pour une opération `needs_attention` (elle attend sans limite).
- **Jamais** dans cette vue (BR-OPS-006) : le plan en JSON, le journal de défaire, un chemin du serveur, une valeur ou un nom de variable, une commande, le nom d'un objet créé.

## `DELETE /api/v1/operations/services/{id}`

- **Droit** : « gérer les services » (ADR-0041) ; **acte d'administration** `OperationCancel` (`0x10`, couvert par l'élévation de 5 minutes : défaut non validé par le propriétaire, seule l'annulation est couverte) : corps `{ "reauth": { … } }` (preuve de la clé du poste liée à l'identifiant de l'opération, mot de passe sauf élévation). Suivi par `Idempotency-Key`.
- Annule l'opération avant son point de non-retour. Une opération en cours passe en `cancelling` (son exécuteur la clôt) ; une opération `accepted` ou `awaiting_decision` passe en `cancelled` tout de suite, son verrou est rendu.
- **Idempotent** : une opération déjà annulée par un compte répond `200` avec la même issue, sans nouvelle entrée de journal.
- Réponse `200` : l'`OperationView` après l'annulation.
- Erreurs : `404 OPERATION_NOT_FOUND` ; `409 TOO_LATE_TO_CANCEL` (le point de non-retour est passé) ; `409 CONFLICT` avec `details.reason = "not_cancellable"` et `details.status` (déjà terminée, à vérifier, en retour arrière) ; `409 CONFLICT` avec `details.reason = "operation_changed"` (elle a bougé pendant la demande : relire) ; `403 FORBIDDEN_ROLE` avec `details.missing = "services"` ; `409 POST_NOT_RECOGNIZED`, `422 WRONG_PASSWORD`, `426` pour la confirmation (voir [security](./security.md)).
- **Journal d'activité** : la réussite s'écrit, dans la transaction du changement, sous l'action du GENRE de l'opération (`service.start`, `service.deploy`, …) avec le compte, la cible « service {nom} : opération annulée ». Un refus ou un échec s'écrit sous `operation.cancel`.

## `POST /api/v1/operations/services/{id}/decision`

- **Droit** : « gérer les services » ; **acte** `OperationDecide` (`0x1D`, **jamais** couvert par l'élévation : le mot de passe est redemandé à chaque réponse, quel que soit le choix ; défaut non validé par le propriétaire, revue de la PR 94) ; **le choix est signé** avec l'identifiant de l'opération. Corps : `{ "choice": "continue" | "cancel" | "revert" | "keep" | "verified", "reauth": { … } }`.
- Applique le choix à une opération `awaiting_decision` ou `needs_attention`. Réponse `200` : l'`OperationView` après la décision.
- Un choix que l'agent ne connaît pas (`unknown`, ou un texte inventé) : `422 VALIDATION_ERROR` (`details.field = "body"`) AVANT toute confirmation, aucun mot de passe essayé. `cancel` suit la règle de l'annulation : `409 TOO_LATE_TO_CANCEL` après le point de non-retour.
- Erreurs : `404 OPERATION_NOT_FOUND` ; `409 DECISION_NOT_OFFERED` avec `details.reason` : `not_waiting` (rien n'est attendu), `not_offered` (choix connu mais absent de `decision.choices`), `not_applicable`, `expired` (attendue depuis 15 minutes ou plus : refusée avant même le balayage, qui l'annule ensuite avec la cause `decision_expired`) ; les erreurs de droit et de confirmation comme ci-dessus.
- « Revenir » démarre un retour arrière (`rolling_back`), jamais un « revenu » sans geste (BR-EDIT-031).
- **Journal d'activité** : réussite sous l'action du genre de l'opération, cible « service {nom} : décision « {poursuivre|annuler|revenir|garder|j'ai vérifié} » » ; refus et échec sous `operation.decide`.

## Message `operation` du sujet `services` du flux

`{ "type": "operation", …les champs d'`OperationView` à plat… }` (`ServicesMessage::Operation`).

- À l'abonnement au sujet `services` : un message pour chaque opération EN COURS que le compte voit ; puis un message à chaque changement (acceptation, étape, progression, décision, fin).
- **Au plus un message par 500 ms et par opération** et par connexion ; les états intermédiaires se fondent, **le dernier état d'une opération n'est jamais perdu** (il part à l'échéance, ou tout de suite si la cadence est remise à zéro). Une rafale de progression est publiée au plus toutes les 100 ms côté agent. Un abonné lent perd les plus anciens changements et reçoit alors ses états retenus, l'état des opérations en cours, puis l'état final des opérations qu'il suivait et qui ont fini entre-temps : il ne ralentit jamais l'agent, et aucune opération ne reste « en cours » à son écran. Les messages d'une même opération arrivent dans l'ordre de leur état (publications sérialisées).
- Filtré par compte comme la lecture (BR-OPS-001), la visibilité étant rejugée à chaque envoi ; le droit est relu toutes les 5 secondes : s'il change pendant le flux, les opérations en cours repartent avec ce que le compte voit maintenant (jusqu'à 5 secondes d'avancement d'un service adopté peuvent encore arriver à un compte à qui on retire le droit).
- Un client qui ne connaît pas ce message lit la trame comme inconnue (`ServerMessage::Unknown`) et l'ignore ; `ServicesMessage` lui-même n'a pas de variante de repli (BR-COMPAT-001 point 2). Le jeu d'un abonné qui revient après une coupure : les opérations en cours arrivent à l'abonnement ; l'issue d'une opération terminée pendant la coupure se relit par `GET` (24 heures).

## `ServiceItem.busy` et `last_action`

Projections du journal (BR-OPS-005), dans `GET /services` et le sujet `services` : `busy { kind, by, since }` (l'opération non terminée qui tient le service, visible de tous) et `last_action { id, kind, by, status, cause?, forced, finished_at? }` (la dernière terminée). Absents tant que le service n'a aucune opération, **et absents pour un compte qui ne peut pas lire les opérations du service** (un service adopté sans le droit « gérer les services » : BR-OPS-001, revue de la PR 94). Pour un service créé par Hearth, visibles de tous.

## Côté client (HRT-84)

`hearth-link` suit ces messages par `OpTracker` (`domain/service_op_tracker.rs`) : un état identique n'est pas annoncé deux fois, l'issue d'une opération terminée l'est une seule fois, et après le retour du lien les opérations que l'agent n'a pas redites à l'abonnement sont relues par `GET` (délai `ops_resync_delay`, 2 s) puis annoncées ou oubliées (`404`). La coquille les range (`service_ops/book.rs`), les rejoue à l'abonnement de la fenêtre et retient chaque issue jusqu'à son acquittement. Commandes : `list_service_operations`, `ack_service_operations`, `decide_service_operation` (acte `0x1D`).

## Bornes

500 opérations terminées gardées au plus et 24 heures (BR-OPS-007) ; 500 opérations en cours rendues à un abonné ; 256 opérations suivies par la cadence d'une connexion ; canal de diffusion de 256 changements.
