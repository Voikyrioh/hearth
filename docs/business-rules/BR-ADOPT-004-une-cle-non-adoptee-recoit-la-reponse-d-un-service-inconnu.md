---
id: BR-ADOPT-004
domaine: ADOPT
titre: Une demande visant un conteneur non adopté reçoit exactement la réponse d'un service inconnu
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-004), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-79
maj: 2026-10-13
---

# BR-ADOPT-004 — Une demande visant un conteneur non adopté reçoit exactement la réponse d'un service inconnu

## Règle
Une demande par la clé d'un conteneur non adopté (valeurs des variables aujourd'hui ; détail, journaux, démarrage, arrêt, redémarrage, quand ces routes existent) reçoit EXACTEMENT la réponse d'un service inconnu : même code, même code d'erreur, même message, même corps. Hearth ne distingue jamais « inconnu » de « non adopté », ni par la réponse, ni par le chemin parcouru : la recherche se fait dans la seule table des services publiés, un conteneur caché y est absent comme un conteneur qui n'existe pas. Pour les journaux, la clé retenue par le filtre est rendue à `may_read` avec l'origine « hors Hearth non adopté » (sans aucune donnée du conteneur), qui la refuse : l'appelant répond `SERVICE_NOT_FOUND`, fermeture normale, comme pour une clé inconnue, sans appel au moteur ; la limite de dix lectures répond la même chose aux deux (la place est prise avant la recherche, pour les deux). Pour un compte sans le droit, le droit est jugé avant la clé (`BR-GRANT-004`) : la réponse est la même pour toute clé.

## Application (code)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::reveal_env (`inner.published.get`)`
- `crates/hearth-agent/src/entrypoint/http/services.rs::env`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::log_target`
- `crates/hearth-agent/src/domain/services/logs.rs::may_read`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::a_request_by_the_key_of_a_container_that_is_not_adopted_gets_the_answer_of_an_unknown_service`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_hidden_container_gets_on_the_logs_route_exactly_the_answer_of_an_unknown_key`
- `crates/hearth-agent/tests/service_logs_ws.rs::the_limit_of_ten_readings_does_not_tell_a_hidden_key_from_an_unknown_one`
- `crates/hearth-agent/tests/services_https.rs::nothing_of_a_container_that_is_not_adopted_leaves_the_agent_by_any_route_message_or_trace`
- `crates/hearth-agent/src/application/services.rs::tests::without_adoption_nothing_is_published_and_every_container_is_counted_stopped_ones_included`

## Cas limites
- Existent aujourd'hui : la révélation des valeurs (`GET /services/{key}/env`) et les journaux (`GET /services/{key}/logs`) ; les routes de détail et d'actes appartiennent à d'autres tickets et devront chercher le service dans la même table publiée.
- Différence restante sur les journaux : aucune dans l'échange (mêmes messages, même fermeture, aucun appel au moteur) ; le temps de réponse n'est pas rendu constant (même chemin de code, aucune attente propre à la clé cachée).
- Le texte de l'erreur est celui d'aujourd'hui (« Ce service n'existe pas. ») ; la spec écrit « Ce service n'existe plus. » (`BR-LIFE-015`), à aligner avec les actes.
- Le journal HTTP de l'agent consigne le chemin de la requête, donc la clé que le client a lui-même envoyée.

## Règles liées
- BR-ADOPT-001, BR-GRANT-004, BR-SVC-009

## Historique
- 2026-10-13 — création (HRT-79, ADR-0043 : ce que l'agent publie, moitié agent, première partie).
- 2026-10-13 — HRT-79 : les journaux passent par le même verdict de visibilité (`may_read`).
- 2026-10-13 — HRT-80 : l'acte d'adopter et celui de relâcher suivent la règle (clé cachée, disparue, inventée ou mal formée : même issue `not_found` ou `released: false`, même entrée de journal sans nom) ; la liste des candidats est la seule lecture qui nomme un caché, réservée au droit (BR-ADOPT-005). Tests : `crates/hearth-agent/tests/service_adoption_https.rs::releasing_what_is_not_adopted_does_nothing_says_the_same_for_every_reason_and_journals_nothing`, `crates/hearth-agent/tests/service_adoption_https.rs::the_result_is_said_service_by_service_a_vanished_one_is_not_adopted_an_adopted_one_is_not_touched`.
