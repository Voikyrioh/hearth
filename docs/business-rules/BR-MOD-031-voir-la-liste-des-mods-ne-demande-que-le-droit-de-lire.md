---
id: BR-MOD-031
domaine: MOD
titre: Voir la liste des mods ne demande que le droit de lire le service
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-031 : Voir la liste des mods ne demande que le droit de lire

## Règle
`GET /services/{key}/mods` est ouverte à tout compte authentifié (niveau `Authenticated`, sans journal, consultation). Ajouter, retirer et appliquer exigent le droit de gérer les services ; appliquer sera l'acte signé `ModsApply` `0x20`, jamais couvert par l'élévation (HRT-258, pas encore livré : cette fiche ne couvre que la lecture pour l'instant).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS` (ligne `GET /services/{key}/mods`, `Access::Authenticated`, `audit: None`)

## Vérification
- Table des classes de route de `entrypoint/http/mod.rs::tests` (`OpenRead`)

## Règles liées
- BR-MOD-011, BR-MOD-020, BR-MOD-025

## Historique
- 2026-11-18 : création (HRT-256), partie lecture.
