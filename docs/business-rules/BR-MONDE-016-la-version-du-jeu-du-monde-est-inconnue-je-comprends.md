---
id: BR-MONDE-016
domaine: MONDE
titre: Hearth ne sait pas avec quelle version du jeu le monde a été enregistré : l'agent exige le geste « Je comprends »
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-016), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-14
---

# BR-MONDE-016 — Hearth ne sait pas avec quelle version du jeu le monde a été enregistré : l'agent exige le geste « Je comprends »

## Règle
La demande de reprise porte `version_understood`. Sans ce geste l'agent refuse (`version_not_understood`), même si le client est modifié. Le geste est signé avec le reste de la demande.

## Application (code)
- `crates/hearth-agent/src/application/fiche_install.rs::FicheInstaller::prepare`
- `crates/hearth-proto/src/api/service_spec.rs::deploy_digest`

## Vérification
- `crates/hearth-agent/tests/world_import_https.rs::a_request_that_the_agent_cannot_honour_creates_nothing`

## Cas limites
- Le texte de la fenêtre est au client (HRT-106).

## Règles liées
- BR-MONDE-014

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
