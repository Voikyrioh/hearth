---
id: BR-ADOPT-002
domaine: ADOPT
titre: Les conteneurs non adoptés sont comptés, sans aucun détail : un nombre, arrêtés compris, le même pour tous les comptes
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-002), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-79
maj: 2026-10-13
---

# BR-ADOPT-002 — Les conteneurs non adoptés sont comptés, sans aucun détail : un nombre, arrêtés compris, le même pour tous les comptes

## Règle
Les conteneurs que le moteur connaît sur le serveur, arrêtés compris, qui ne sont ni créés par Hearth ni adoptés sont comptés : `engine.external_count` (sur `GET /services`, `POST /engine/recheck` et dans le lot du sujet `services` quand il change). Aucun autre détail : ni nom, ni image, ni port, ni variable, ni commande, ni journal, ni dossier monté, ni état. Le nombre est le même pour tous les comptes, quel que soit leur rôle et leur droit.

On compte des conteneurs, pas des services : un conteneur arrêté, reste d'une recréation, compte tant qu'aucun service publié ne le porte. Dans les états du moteur sans liste (absent, non pris en charge, désactivé) le registre est vidé et le compte vaut 0. Un champ absent vaut 0 (agent d'avant l'adoption).

## Application (code)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::publish (`inner.external`)`
- `crates/hearth-agent/src/application/services.rs::EngineStatus::external_count`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::engine_view`
- `crates/hearth-proto/src/api/services.rs::EngineView::external_count`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::on_a_server_whose_containers_are_all_outside_hearth_the_list_is_empty_and_they_are_counted`
- `crates/hearth-agent/src/application/services.rs::tests::without_adoption_nothing_is_published_and_every_container_is_counted_stopped_ones_included`
- `crates/hearth-agent/src/application/services.rs::tests::an_adopted_container_is_published_by_its_key_and_the_others_stay_counted`
- `crates/hearth-proto/src/api/services.rs::tests::the_adoption_fields_are_absent_until_filled_and_old_clients_ignore_them`

## Cas limites
- Le compte bouge à l'apparition et à la disparition d'un conteneur caché, et seulement là : c'est la seule chose qu'un conteneur caché fasse observer, et c'est voulu (message de conflit de port compréhensible, ADR-0043 « Le compte seul renseigne déjà un peu »).
- Un conteneur dont l'identifiant n'a pas la forme d'une clé n'est pas listable : il est compté comme les autres.

## Règles liées
- BR-ADOPT-001, BR-ADOPT-023, BR-ENGINE-004

## Historique
- 2026-10-13 — création (HRT-79, ADR-0043 : ce que l'agent publie, moitié agent, première partie).
