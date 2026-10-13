---
id: BR-ADOPT-009
domaine: ADOPT
titre: Adopter n'écrit rien sur le service ni sur le serveur : une ligne dans la base de l'agent, aucune étiquette, aucun appel d'écriture au moteur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-009), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-13
---

# BR-ADOPT-009 — Adopter n'écrit rien sur le service ni sur le serveur : une ligne dans la base de l'agent, aucune étiquette, aucun appel d'écriture au moteur

## Règle
L'adoption est une décision de Hearth mémorisée par l'agent : une ligne de `adopted_services` (clé, nom, unité, compte, date), écrite dans la même transaction que l'entrée du journal d'activité. Le port du moteur (`ContainerEngine`) n'a AUCUN geste d'écriture (sonde, liste, inspection, événements, variables, journaux) : l'acte ne peut donc ni poser une étiquette, ni redémarrer, ni recréer un conteneur ; le superviseur de services ne fait que de la détection. Avant et après l'adoption, ce que le moteur dit du conteneur est identique.

## Application (code)
- `crates/hearth-agent/src/application/ports/container_engine.rs::ContainerEngine`
- `crates/hearth-agent/src/application/ports/adoption_tx.rs::AdoptionTx`
- `crates/hearth-agent/src/infrastructure/sqlite/adoption_repo.rs`

## Vérification
- `crates/hearth-agent/tests/service_adoption_https.rs::adopting_changes_nothing_on_the_service_publishes_it_and_journals_one_entry_per_service`

## Cas limites
- Sur un vrai moteur et un vrai systemd (étape CI `supervised`, `tests/engine_supervised.rs`), l'absence de changement de l'unité et du conteneur ne se prouve qu'en CI : le faux moteur prouve la forme (aucun geste d'écriture n'existe), pas le comportement de Docker.

## Règles liées
- BR-ADOPT-014, BR-ADOPT-017

## Historique
- 2026-10-13 — création (HRT-80, ADR-0043 : l'acte d'adopter et de relâcher, moitié agent et protocole).
