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
Approche retenue (revue Stephen, rounds 1 et 2) : une porte de clôture PAR SERVICE (`application/closing_gates.rs`, table clé vers `RwLock<()>` async, entrée retirée au `Drop` quand plus personne ne la tient ni ne l'attend). Toute clôture la prend en lecture AVANT son commit et la tient jusqu'à la fin de `publish` (verrous rendus, projection, annonce) : `close`/`finish_with`, `move_to` vers un statut terminal, l'expiration de `sweep` (nettoyage disque après, hors porte), l'annulation et la décision de `ServiceOpAccess` (porte TOUJOURS prise avant le `begin`, sans dépendre du statut lu hors transaction : un arrêt de jeu peut passer de `running` à `awaiting_decision` entre la lecture et la transition ; garde passée à `settle`). Les transitions non terminales de `ServiceOps` ne la prennent pas. `ServiceLifecycle::request` appelle `wait_closed` UNE fois (écriture éphémère, au plus 5 s : `CLOSING_GATE_WAIT`, au moins égal au `busy_timeout` sqlite, une clôture qui attend la base ne dépasse pas la porte ; round 3), juge `ServiceOps::busy`, puis `accept_after_wait` (pas de seconde attente) ; `accept` attend aussi une fois. À l'échéance : `warn!` sans valeur, on juge quand même. `ServiceOps` porte seul l'état « occupé ». Les déploiements en profitent via `accept`.
Coût : un verrou court sur la table et une allocation par clôture ; une clôture lente sur A ne fait jamais attendre B (seul compromis restant : plusieurs demandes sur le MÊME service derrière une clôture attendent ensemble, au plus l'échéance).
Absence d'interblocage : l'ordre est toujours porte puis transaction ; sous la porte une clôture n'attend que le journal et `publishing`, jamais une autre porte ; chaque attente a une échéance. « Jamais refusé » ne vaut que dans l'échéance : au-delà on juge sur l'état courant. Sans clôture en cours, l'attente est nulle (monter l'échéance ne coûte rien).
Liste des services : `ServiceItem.busy` lit la projection SANS attendre la porte (volontaire, la liste ne bloque pas) ; pendant une clôture elle peut afficher « occupé » un instant, la publication qui suit la rafraîchit.
Limite : `settle` sans garde ne couvre que libération et annonce (commit hors de portée).
Rejeté :
- Rendre le verrou avant le commit : si le commit échoue, l'opération reste `running` sans verrou, et le repli « clôture sans entrée » rouvre le service.
- Réparer à l'acceptation en relisant le journal (première version : `release_stale_locks`, `settle_if_terminal`) : double annonce du même terminal, qui peut devancer le `pending.publish(trail)` du fil de clôture (ordre trace/flux de FIX-01M5NYA5W781YW5NFQ7W1JGR98), un `journal.get` par détenteur à chaque 409.
- Porte globale (round 1) : une clôture lente sur un service faisait attendre jusqu'à l'échéance toutes les demandes de tous les services.
Tests déterministes (`service_ops_journal.rs`, `Notify`, aucun sommeil, échéance injectable `ServiceOps::with_closing_wait`) : la clôture est aussi parquée par l'expiration de `sweep`, l'annulation et la décision de `ServiceOpAccess` ; la clôture est arrêtée après son commit, par `close` (transaction ralentie) et par `move_to` (journal ralenti) ; `request` (attente puis `busy`) et `accept` attendent puis réussissent ; un abonné reçoit l'issue une seule fois malgré un accept concurrent ; A lent n'affecte pas B ; une clôture bloquée au-delà de l'échéance et une opération réellement en cours donnent 409. `the_next_action_is_never_refused_once_the_previous_outcome_can_be_read` : 5 passes consécutives.

## Références
- Ticket : HRT-101
- Lié : FIX-01M5NYA5W781YW5NFQ7W1JGR98
