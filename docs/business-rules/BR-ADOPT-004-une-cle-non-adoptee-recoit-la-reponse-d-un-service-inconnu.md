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
Une demande par la clé d'un conteneur non adopté (valeurs des variables aujourd'hui ; détail, journaux, démarrage, arrêt, redémarrage, quand ces routes existent) reçoit EXACTEMENT la réponse d'un service inconnu : même code, même code d'erreur, même message, même corps. Hearth ne distingue jamais « inconnu » de « non adopté », ni par la réponse, ni par le chemin parcouru : la recherche se fait dans la seule table des services publiés, un conteneur caché y est absent comme un conteneur qui n'existe pas. Pour un compte sans le droit, le droit est jugé avant la clé (`BR-GRANT-004`) : la réponse est la même pour toute clé.

## Application (code)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::reveal_env (`inner.published.get`)`
- `crates/hearth-agent/src/entrypoint/http/services.rs::env`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::a_request_by_the_key_of_a_container_that_is_not_adopted_gets_the_answer_of_an_unknown_service`
- `crates/hearth-agent/src/application/services.rs::tests::without_adoption_nothing_is_published_and_every_container_is_counted_stopped_ones_included`

## Cas limites
- Seule la révélation des valeurs (`GET /services/{key}/env`) existe dans cette version ; les routes de détail, de journaux et d'actes appartiennent à d'autres tickets et devront chercher le service dans la même table publiée.
- Le texte de l'erreur est celui d'aujourd'hui (« Ce service n'existe pas. ») ; la spec écrit « Ce service n'existe plus. » (`BR-LIFE-015`), à aligner avec les actes.
- Le journal HTTP de l'agent consigne le chemin de la requête, donc la clé que le client a lui-même envoyée.

## Règles liées
- BR-ADOPT-001, BR-GRANT-004, BR-SVC-009

## Historique
- 2026-10-13 — création (HRT-79, ADR-0043 : ce que l'agent publie, moitié agent, première partie).
