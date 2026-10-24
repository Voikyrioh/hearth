---
id: FIX-01M5P0Q3T8ZK6V2R9XH4DWB7NC
titre: L'action suivante d'un service était refusée (409 SERVICE_BUSY) juste après avoir lu l'issue terminale de la précédente (HRT-101)
date_découverte: 2026-10-24
date_correction: 2026-10-24
---

# FIX-01M5P0Q3T8ZK6V2R9XH4DWB7NC : Action suivante refusée en 409 SERVICE_BUSY après lecture de l'issue terminale (HRT-101)

## Symptôme
`service_lifecycle_https::the_next_action_is_never_refused_once_the_previous_outcome_can_be_read` (ajouté par FIX-01M5NYA5W781YW5NFQ7W1JGR98) échouait en 409 SERVICE_BUSY dans 2 suites complètes sur 3 (passait seul 25/25).

## Reproduction
Suite complète sous charge : lire `GET /operations/services/{id}` jusqu'à l'issue terminale, puis lancer aussitôt l'action suivante du même service.

## Cause root
L'index de projection (BR-LIFE-013, `busy_of`) est mis à jour par `publish`, donc APRÈS la libération : même fenêtre, et c'est elle qui refusait en pratique (le message portait le `by` et le `kind` de l'index). Les chemins de clôture (`ServiceOps::close`, `move_to`, `settle`) font : commit du statut terminal, PUIS `release_all_of`, PUIS annonce. `GET` lit le journal persisté : entre le commit et la libération, il voit l'issue alors que le verrou mémoire est encore tenu. `accept` ne regarde que le verrou mémoire (puis l'index unique en base) et refusait. Le correctif précédent avait fixé l'ordre « verrous avant annonce » (flux), pas la fenêtre commit -> libération vue par le GET.

## Impacté
Tout client qui enchaîne une action dès que l'issue est lisible (client Hearth, scripts, tests).

## Workaround
Réessayer.

## Correction
Approche retenue (revue Stephen, round 1) : une porte de clôture (`RwLock<()>` async dans `ServiceOps`). `close`, `move_to` et `settle` la prennent en lecture (plusieurs clôtures en parallèle) du commit du statut terminal jusqu'à la fin de `publish` (verrous rendus, projection, annonce). `accept` et `ServiceOps::busy` l'attendent une fois (écriture éphémère, `CLOSING_GATE_WAIT` = 1 s) avant de juger « occupé ». À l'échéance on juge quand même : une opération réellement en cours donne toujours 409, une clôture bloquée ne gèle pas le service. `ServiceOps` porte seul l'état « occupé » (`busy`) : `ServiceLifecycle::request` ne lit plus l'index. Les déploiements en profitent via `accept`. Aucun interblocage : la porte n'est tenue que par des clôtures, qui n'attendent jamais la porte (elles n'attendent que le journal et `publishing`). Limite : pour `settle`, le commit est fait par l'appelant (transaction du journal d'activité) avant l'appel ; la porte ne couvre que libération et annonce.
Rejeté :
- Rendre le verrou avant le commit : si le commit échoue, l'opération reste `running` sans verrou, et le repli « clôture sans entrée » rouvre le service.
- Réparer à l'acceptation en relisant le journal (`release_stale_locks`, `settle_if_terminal`, première version) : publie deux fois le même terminal, peut devancer le `pending.publish(trail)` du fil de clôture (ordre trace/flux de FIX-01M5NYA5W781YW5NFQ7W1JGR98), un `journal.get` par détenteur à chaque 409, deux mécanismes pour une seule fenêtre.
Tests (`service_ops_journal.rs`, journal à pause après commit) : chemin `busy`/projection, `accept` qui attend, un abonné reçoit l'issue une seule fois, 409 d'une opération en cours et d'une clôture plus longue que la porte. `the_next_action_is_never_refused_once_the_previous_outcome_can_be_read` : 5 passes consécutives.

## Références
- Ticket : HRT-101
- Lié : FIX-01M5NYA5W781YW5NFQ7W1JGR98
