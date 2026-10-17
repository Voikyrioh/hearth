---
id: BR-DEPLOY-008
domaine: DEPLOY
titre: Un port tenu par un conteneur en marche ou un programme du serveur bloque le lancement ; un port réservé par un service arrêté avertit et se coche ; un conteneur hors Hearth n'est jamais nommé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-008), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-008 — Un port tenu par un conteneur en marche ou un programme du serveur bloque le lancement ; un port réservé par un service arrêté avertit et se coche ; un conteneur hors Hearth n'est jamais nommé

## Règle
Le formulaire vérifie le port à la saisie (`POST /deployments/check`) et l'agent le revérifie au lancement (`409 PORT_IN_USE`, `details.port`, `details.status`, `details.holder`, `details.suggested`). Un conteneur non adopté répond « Port tenu par un conteneur créé hors de Hearth. » sans son nom (BR-ADOPT-022). Un programme du serveur est nommé quand l'agent le voit (`/proc/net`, nom cherché pour le seul port en conflit) ; un écouteur `docker-proxy` appartient à son conteneur.

## Application (code)
- `crates/hearth-agent/src/domain/services/ports.rs::verdict`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::check_ports_for`
- `crates/hearth-agent/src/infrastructure/host/listening_ports.rs`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::port_claim`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_port_held_by_a_container_a_program_or_the_agent_is_refused_with_a_suggestion`
- `crates/hearth-agent/tests/service_deploy_https.rs::a_port_reserved_by_a_stopped_service_needs_the_box_to_be_ticked`
- `crates/hearth-agent/src/domain/services/ports.rs::tests::a_stopped_container_only_reserves_the_port`

## Cas limites
- La liste des sockets illisible : l'agent ne prétend rien, la collision se découvre au démarrage du conteneur (BR-DEPLOY-019).
- Les ports d'un service en réseau hôte ne sont pas listés (limite de `Observation`).

## Règles liées
BR-ADOPT-022, BR-DEPLOY-009, BR-DEPLOY-019

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
