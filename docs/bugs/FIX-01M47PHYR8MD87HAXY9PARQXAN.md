---
id: FIX-01M47PHYR8MD87HAXY9PARQXAN
titre: La patience de la surveillance concluait « échec du superviseur » même quand l'échange des binaires avait eu lieu
date_découverte: 2026-10-06
date_correction: 2026-10-06
---

# FIX-01M47PHYR8MD87HAXY9PARQXAN : La patience de `watch` est hors de `classify_orphan`

## Symptôme
Un superviseur disparaît sans écrire de résultat (tué, planté) alors que les binaires sont déjà échangés. L'agent qui le surveille (le nouvel agent, démarré par le superviseur, qui reprend la surveillance au démarrage) attend 30 secondes, puis écrit `failed` / `supervisor_launch` : « le superviseur n'a pas pu être lancé ». C'est faux (il a travaillé, le nouvel agent tourne), le résultat est consigné au journal avec ce mensonge, et le dépôt est nettoyé : `job.json`, `state.json` et la **copie de la base** sont retirés, donc plus aucune reprise ni retour arrière ne sont possibles pour cette mise à jour (la sauvegarde de l'ancien binaire reste, orpheline).

## Reproduction
Test rouge avant correctif : `tests/update_use_cases.rs::a_supervisor_that_dies_after_the_swap_is_taken_over_never_declared_failed` (banc à patience de 30 ms : superviseur vivant à la reprise de la surveillance, puis disparu ; travail avec sauvegarde de l'ancien binaire et copie de la base). Avant : un échec écrit, aucun superviseur de reprise lancé. Jamais observé sur une vraie machine.

## Cause root
`UpdateService::watch` : passé `SUPERVISOR_PATIENCE`, la tâche écrivait directement un résultat d'échec sans relire les traces, alors que `domain::update::classify_orphan` (BR-UPDATE-028) sait dire jusqu'où le travail est allé. Deux règles de conclusion pour le même état, et celle de la patience ne regardait rien.

## Impacté
Production (agent root) : résultat faux au journal et au client, perte de la possibilité de reprise après un superviseur tué. Conditions : superviseur disparu sans résultat **après** l'échange.

## Workaround
Aucun avant correctif : reprise à la main (runbook).

## Correction
Passé la patience, la surveillance relit les traces et applique `classify_orphan` (`give_up_on_supervisor`) : seul « rien n'a été échangé » (`None`, `BeforeLaunch`, `LaunchedNoSwap`) reste `failed` / `supervisor_launch` ; les autres cas suivent la conclusion du démarrage (`conclude_orphan` : reprise, réussite non écrite, retour déjà fait, version tierce, trace illisible). La patience est un réglage (`Timing::patience`, 30 s par défaut) pour que les tests ne l'attendent pas. Rejeté : allonger la patience (ne change pas la règle).

## Règles
- BR-UPDATE-028 (conclusion d'un travail laissé en cours) : étendue à la patience de la surveillance.

## Non-régression
- `tests/update_use_cases.rs::a_supervisor_that_dies_after_the_swap_is_taken_over_never_declared_failed`
- `tests/update_use_cases.rs::a_supervisor_that_never_shows_up_ends_as_a_launch_failure_after_the_patience` (le cas qui reste un échec du lancement)

## Références
- Ticket : HRT-17 (suivis de review du lot agent), tâche T27
- Code : `crates/hearth-agent/src/application/update.rs` (marqueur `FIX:01M47PHYR8MD87HAXY9PARQXAN`)
