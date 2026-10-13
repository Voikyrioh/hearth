---
id: FIX-01M4SFCQ7N5TN8VC21KBQR87PP
titre: Au retour du moteur, l'ancienne liste repartait sous « prêt » sans marque
date_découverte: 2026-10-13
date_correction: 2026-10-13
---

# FIX-01M4SFCQ7N5TN8VC21KBQR87PP : l'ancienne liste réapparaît fraîche au retour du moteur (HRT-72, N1)

Ticket : HRT-72 (revue de la PR 77, troisième passe). Sévérité : majeur (atteignable dès que le client de HRT-62 lit le fil). Zones : `crates/hearth-agent/src/application/services.rs` (`note_published`, `list_degraded`, `resync`).

## Symptôme
Moteur absent puis de retour : la liste d'avant la disparition repartait sur le fil sous l'état « prêt », `list_degraded` à faux, jusqu'à la fin de la relecture (jusqu'à 30 s sur un moteur lent). Même fenêtre au retour d'un état inaccessible (la liste gardée, grisée, passait à « prête » avant d'être relue), et au premier démarrage (liste vide sous « prêt » non marquée).

## Reproduction
Test `application::services::tests::a_list_from_before_the_engine_vanished_is_never_shown_as_fresh_when_it_returns` : service lu, moteur absent publié, moteur de retour, sonde appliquée SANS relecture. Rouge avant le correctif (constaté : l'assertion « aucun service d'avant la disparition » échoue, la liste d'avant est publiée), vert après.

## Cause root
Le registre n'était jamais vidé dans les états sans liste : `commit` ne faisait que masquer la liste (`keeps_no_list`), `containers` restait en mémoire. Au retour, `apply_probe` publiait « prêt » et `commit` AVANT la relecture : l'ancienne liste repartait. Aucune marque ne disait « pas relue depuis le retour ».

## Impacté
Le fil `services` (instantané et lots), `GET /services`. Aucun client ne lisait encore ces champs (jamais publié).

## Workaround
Aucun.

## Correction
`note_published` (appelé à chaque publication d'état) vide vraiment le registre dans les états sans liste (conteneurs, fenêtres, date de lecture) et lève `list_loaded` dès que l'état publié n'est plus « prêt ». `list_degraded` est vrai tant que `list_loaded` est faux sous « prêt » ou un état inaccessible : la première relecture complète qui suit le retour (`resync`) le lève. Alternative rejetée : ne pas vider et masquer plus tard (la phrase « le registre se vide » de BR-SVC-018 aurait continué d'être fausse). La liste gardée des états inaccessibles reste disponible, marquée puis datée (spec : « grisée, datée »).

## Règles
- BR-SVC-001, BR-SVC-018 (mises à jour).
