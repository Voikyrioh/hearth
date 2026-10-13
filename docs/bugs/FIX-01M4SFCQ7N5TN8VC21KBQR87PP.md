---
id: FIX-01M4SFCQ7N5TN8VC21KBQR87PP
titre: Au retour du moteur, l'ancienne liste repartait sous « prêt » sans marque
date_découverte: 2026-10-13
date_correction: 2026-10-13
---

# FIX-01M4SFCQ7N5TN8VC21KBQR87PP : l'ancienne liste réapparaît fraîche au retour du moteur (HRT-72, N1)

Ticket : HRT-72 (revue de la PR 77, troisième passe). Sévérité : majeur (atteignable dès que le client de HRT-62 lit le fil). Zones : `crates/hearth-agent/src/application/services.rs` (`note_observed`, `note_published`, `list_degraded`, `list_fresh`, `resync`).

## Symptôme
Moteur absent puis de retour : la liste d'avant la disparition repartait sur le fil sous l'état « prêt », `list_degraded` à faux, jusqu'à la fin de la relecture (jusqu'à 30 s sur un moteur lent). Même fenêtre au retour d'un état inaccessible (la liste gardée, grisée, passait à « prête » avant d'être relue), et au premier démarrage (liste vide sous « prêt » non marquée).

## Reproduction
Test `application::services::tests::a_list_from_before_the_engine_vanished_is_never_shown_as_fresh_when_it_returns` : service lu, moteur absent publié, moteur de retour, sonde appliquée SANS relecture. Rouge avant le correctif (constaté : l'assertion « aucun service d'avant la disparition » échoue, la liste d'avant est publiée), vert après.

## Cause root
Le registre n'était jamais vidé dans les états sans liste : `commit` ne faisait que masquer la liste (`keeps_no_list`), `containers` restait en mémoire. Au retour, `apply_probe` publiait « prêt » et `commit` AVANT la relecture : l'ancienne liste repartait. Aucune marque ne disait « pas relue depuis le retour ».

## Deuxième passe (revue de la PR 78, R1)
Le premier correctif suivait l'état PUBLIÉ alors que la relecture suit l'état OBSERVÉ. Enchaînement : absent publié (registre vidé) ; retour dans la fenêtre (« prêt » retenu, mais la boucle relit et remplit le registre) ; seconde disparition avant l'échéance (état observé = état publié, rien n'est publié, rien n'est revidé) ; retour : « prêt » est publié et la liste d'avant la seconde disparition repart sans marque, datée de maintenant. Test rouge constaté avant, vert après : `a_list_read_during_a_held_back_return_is_not_kept_across_a_second_loss` (et la variante `a_list_not_read_since_a_held_back_loss_is_never_dated_now`). Cause : deux sources de vérité (publié, observé). Correctif : `list_loaded` est remis à faux dès que le moteur est OBSERVÉ non prêt (`note_observed`, à chaque sonde), le registre est revidé à chaque sonde et à chaque publication quand l'état publié ne garde pas de liste, `list_fresh` (donc `list_current_at`) exige l'état observé prêt ET la liste relue, `list_current_at` est absent dans les états sans liste, et une lecture qui finit après que le moteur a été vu partir ne vaut pas relecture. La marque `list_degraded` reste soumise à l'anti-clignotement (`gap_shown` : due seulement si le client a vu un état sans « prêt »), sinon une coupure retenue la ferait basculer (`the_engine_state_changes_in_time_and_never_flickers`, constaté rouge une fois sur deux avec une marque calée sur l'observé). Réserve : sous un état inaccessible publié, les va-et-vient ne défont pas une relecture déjà faite (même raison).

## Impacté
Le fil `services` (instantané et lots), `GET /services`. Aucun client ne lisait encore ces champs (jamais publié).

## Workaround
Aucun.

## Correction
`note_published` (appelé à chaque publication d'état) vide vraiment le registre dans les états sans liste (conteneurs, fenêtres, date de lecture). `list_degraded` est vrai tant que la liste n'est pas relue sous « prêt » publié, une fois que le client a vu un état sans « prêt » (jamais pour la seule raison « pas relue » dans un état inaccessible) : la première relecture complète qui suit le retour (`resync`) le lève. Alternative rejetée : ne pas vider et masquer plus tard (la phrase « le registre se vide » de BR-SVC-018 aurait continué d'être fausse). La liste gardée des états inaccessibles reste disponible, marquée puis datée (spec : « grisée, datée »).

## Règles
- BR-SVC-001, BR-SVC-018 (mises à jour).
