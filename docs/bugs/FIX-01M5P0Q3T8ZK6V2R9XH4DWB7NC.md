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
Choix : réparer côté acceptation plutôt que rendre le verrou avant le commit. Rendre le verrou avant le commit rouvrirait le service si le commit échoue (opération encore `running` sans verrou) et casserait le repli « clôture sans entrée ». Dans `ServiceOps::accept` (`application/service_ops.rs`), si la prise des verrous est refusée, `release_stale_locks` relit au journal chaque opération qui tient un verrou, un nom ou la place lourde (`LockTable::holders`, `domain/ops/locks.rs`), rend ceux dont l'opération est déjà terminale en base (un terminal ne tient plus rien, la libération est idempotente), puis réessaie UNE fois. Une opération encore en cours garde son verrou et le refus est inchangé. Même fenêtre côté index : `ServiceLifecycle::request` voit « occupé » par la projection (mise à jour après la libération) ; si le journal dit l’opération terminale, `ServiceOps::settle_if_terminal` rend les verrous et met la projection à jour avant de trancher. Couvre aussi `move_to`, `settle` et les chemins externes. Test déterministe : `service_ops_journal::a_lock_whose_operation_is_already_terminal_in_the_journal_never_refuses_the_next_action`.

## Références
- Ticket : HRT-101
- Lié : FIX-01M5NYA5W781YW5NFQ7W1JGR98
