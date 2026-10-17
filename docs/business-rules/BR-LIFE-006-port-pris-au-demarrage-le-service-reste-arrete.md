---
id: BR-LIFE-006
domaine: LIFE
titre: Si le démarrage échoue parce qu'un port demandé est déjà pris, le service reste arrêté et la cause est dite
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-006), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-006 — Si le démarrage échoue parce qu'un port demandé est déjà pris, le service reste arrêté et la cause est dite

## Règle
Le port du moteur qui répond « port déjà alloué » (ou « address already in use », reconnu dans le texte par l'adaptateur qui ne le laisse jamais sortir) devient `EngineError::PortInUse`, puis l'opération `failed` avec la cause `port_in_use` et l'entrée « port occupé » au journal d'activité. Le conteneur n'est pas touché : il reste arrêté. Qui tient le port (autre service, programme du serveur) se lit par le registre (`port_holder`, BR-ADOPT-022) et se dit côté client. Pour un service géré par le système, un échec de `systemctl start` est `engine_refused` (le système ne dit pas quel port).

## Application (code)
- `crates/hearth-agent/src/infrastructure/engine/errors.rs::{translate, port_taken}`
- `crates/hearth-agent/src/application/ports/container_engine.rs::EngineError::PortInUse`
- `crates/hearth-agent/src/application/service_lifecycle.rs::OrderFailure`

## Vérification
- `crates/hearth-agent/tests/service_lifecycle_https.rs::a_start_that_meets_a_taken_port_leaves_the_service_stopped_with_the_cause`
- `crates/hearth-agent/src/infrastructure/engine/errors.rs::tests::a_taken_port_is_recognised_in_the_text_and_the_text_is_dropped`
- `crates/hearth-agent/tests/support/engine_contract.rs::writes (port pris, trois exécutants)`

## Cas limites
- Limite assumée : les formulations du texte du moteur (Docker, Podman) sont confirmées par le job `engine` de la CI (conflit de port réel).

## Règles liées
- BR-ADOPT-022, BR-LIFE-012, BR-LIFE-018

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
