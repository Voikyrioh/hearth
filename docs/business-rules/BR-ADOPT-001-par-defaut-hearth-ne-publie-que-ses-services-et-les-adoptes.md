---
id: BR-ADOPT-001
domaine: ADOPT
titre: Par défaut Hearth ne publie et ne pilote que les services qu'il a créés et les services adoptés ; rien d'un conteneur non adopté ne sort de l'agent
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-001), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-79
maj: 2026-10-13
---

# BR-ADOPT-001 — Par défaut Hearth ne publie et ne pilote que les services qu'il a créés et les services adoptés ; rien d'un conteneur non adopté ne sort de l'agent

## Règle
Par défaut, Hearth ne publie et ne pilote que les services qu'il a créés (clé `h_`) et les services adoptés. La liste (`GET /services`, sujet `services`), le détail, les révélations et toute route qui désigne un service par sa clé ne portent que sur ces services. Hearth continue de LIRE tous les conteneurs du serveur pour son fonctionnement interne (conflits de ports et de noms, total des ressources, candidats à l'adoption) : le registre complet reste en mémoire, mais rien d'un conteneur non adopté ne sort de l'agent.

Un conteneur non adopté ne sort sous aucune forme : ni son nom, ni son image, ni un port, ni une étiquette, ni le nom de son unité, ni une variable, ni sa commande, ni un journal, ni dans un message d'erreur, ni dans une trace à aucun niveau. Le filtre est appelé à UN seul endroit, à la fin de chaque recalcul du registre (`ServicesRegistry::publish`, qui applique `visibility_of`) : l'instantané, les lots de changements et la recherche par clé ne voient que ce qui l'a passé. Un changement qui ne touche qu'un conteneur caché n'avance pas `rev` et ne fait partir aucun message.

Sur un serveur dont tous les conteneurs sont hors Hearth et non adoptés, la liste est vide (tant que l'adoption, HRT-80, n'est pas livrée).

## Application (code)
- `crates/hearth-agent/src/domain/services/visibility.rs::visibility_of`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::publish`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::reload_adopted`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::{snapshot, delta, service_item}`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::nothing_of_a_container_that_is_not_adopted_leaves_the_agent_by_any_route_message_or_trace`
- `crates/hearth-agent/tests/services_https.rs::on_a_server_whose_containers_are_all_outside_hearth_the_list_is_empty_and_they_are_counted`
- `crates/hearth-agent/src/application/services.rs::tests::without_adoption_nothing_is_published_and_every_container_is_counted_stopped_ones_included`
- `crates/hearth-agent/src/application/services.rs::tests::a_change_to_a_hidden_container_moves_neither_the_revision_nor_the_deltas`
- `crates/hearth-agent/src/domain/services/visibility.rs::tests::a_container_is_hidden_until_it_is_adopted_by_its_key`

## Cas limites
- Observable malgré tout, par choix ou par nature : le compte des non gérés (`BR-ADOPT-002`, voulu) et sa variation à l'apparition ou à la disparition d'un conteneur ; le drapeau `list_degraded`, qu'un conteneur caché illisible lève comme n'importe quel conteneur (un bit sur la fraîcheur de la liste, sans nom) ; la date de la dernière lecture de la liste ; la charge de l'agent. Le test sentinelle les consigne.
- Une trace au niveau le plus bavard contient les instructions SQL de la base (texte seul, sans paramètre) : aucune ne porte un conteneur non adopté, puisque la base ne retient que des services adoptés.
- Les conteneurs sont lus par une bibliothèque de transport dont les traces sont filtrées sous `warn` (`TransportGuard`, `infrastructure/logging.rs`) : c'est ce filtre qui garde ce qu'elle écrit.

## Règles liées
- BR-ADOPT-002, BR-ADOPT-004, BR-SVC-001, BR-SVC-003, BR-ENGINE-004

## Historique
- 2026-10-13 — création (HRT-79, ADR-0043 : ce que l'agent publie, moitié agent, première partie).
