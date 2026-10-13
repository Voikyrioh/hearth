---
id: FIX-01M4SFCRJWX8KMBC1ZK9ZWETTW
titre: La date de la liste sur le fil faisait surestimer son âge, sans règle pour l'écart d'horloge
date_découverte: 2026-10-13
date_correction: 2026-10-13
---

# FIX-01M4SFCRJWX8KMBC1ZK9ZWETTW : `list_read_at` ne date pas la fraîcheur de la liste (HRT-72, N2)

Ticket : HRT-72 (revue de la PR 77, troisième passe). Sévérité : majeur pour HRT-62. Zones : `application/services.rs` (`list_current_at`, `track_staleness`, `status_of`), `entrypoint/services_wire.rs::engine_view`, `hearth-proto/src/api/services.rs::EngineView`.

## Symptôme
Un agent lancé il y a trois jours dont le moteur cesse de répondre envoyait `list_read_at` d'il y a trois jours alors que la liste était exacte il y a deux secondes : le client aurait écrit « Liste du moteur d'il y a 3 jours ». Un seul conteneur durablement illisible gelait la date. Un client ne distinguait pas « jamais lue » d'« agent ancien ». Rien ne disait comment calculer l'âge malgré l'écart d'horloge entre client et serveur.

## Reproduction
`tests::the_date_on_the_wire_is_the_last_moment_the_list_was_up_to_date` : trois jours plus tard la liste suivie en direct est à jour (âge zéro) alors que `list_read_at` date de trois jours ; le moteur s'arrête, la date devient l'instant de la perte et ne rajeunit pas. Aussi `tests::a_list_that_stops_being_followed_is_dated_from_the_moment_it_was_noticed`, `tests::a_missing_engine_has_no_list_to_date`, et côté fil `the_list_age_fields_tell_a_never_read_list_from_an_old_agent`. Les champs de date nouveaux n'existaient pas avant : ces tests n'ont pas d'état « rouge » avant, ils fixent la sémantique.

## Cause root
`list_read_at` n'est posé que par une relecture complète sans conteneur illisible : c'est la fin de la dernière relecture, pas le dernier instant où la liste était juste. Le fil ne portait ni l'heure de l'agent ni de marque « aucune liste lue ».

## Impacté
Futur affichage « Liste du moteur d'il y a {âge} » (HRT-62), jamais publié.

## Workaround
Aucun.

## Correction
Deux champs facultatifs de `engine` (`serde(default)`, rien de renommé ni retiré) : `at` (heure de l'agent à la fabrication de la vue) et `list_current_at` (dernier moment où la liste était à jour : `at` si elle l'est, sinon l'instant où elle a cessé de l'être, posé par `track_staleness` à chaque `commit`). L'âge est `max(0, at - list_current_at)` plus le temps écoulé chez le client depuis la réception : deux dates de l'agent, jamais l'horloge du client comparée à celle de l'agent. Précédent du dépôt : « Vu il y a » (`StaleStamp`, `lastContactAt`) part de l'instant de réception chez le client, et les courbes prennent leur « maintenant » dans les échantillons de l'agent (`dashboard/series.ts`), sans comparer deux horloges. `at` présent et `list_current_at` absent : jamais lue ; `at` absent : agent ancien. `list_read_at` est conservé (information).

## Règles
- BR-SVC-001, BR-SVC-018 (mises à jour), `docs/open-api/services.md`.
