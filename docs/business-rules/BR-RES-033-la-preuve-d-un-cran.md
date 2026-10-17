---
id: BR-RES-033
domaine: RES
titre: Un cran de joueurs n'accepte que « mesuré » ou « chiffre de l'éditeur » avec une adresse https
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-paliers-de-memoire.md (BR-RES-033), HRT-122
maj: 2026-10-17
---

# BR-RES-033 — Un cran de joueurs n'accepte que « mesuré » ou « chiffre de l'éditeur » avec une adresse https

## Règle
Chaque cran déclaré porte sa preuve sur l'échelle de BR-FICHE-008 : `"measured"` ou `{ publisher = "https://..." }`. « Mesuré au repos » ne dit rien d'un serveur plein et « non mesuré » n'a pas de chiffre : tous deux sont refusés (`CranPreuveInterdite`). Une adresse absente ou sans `https` est refusée (I5, `AdresseNonHttps`) ; Hearth l'affiche, ne la contacte jamais.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::check_memory_tiers`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_memory.rs::a_tier_refuses_idle_and_unmeasured_proof`
- `crates/hearth-agent/src/domain/catalog/tests_memory.rs::a_publisher_tier_needs_an_https_address`

## Cas limites
- La forme de la preuve est celle de HRT-113 (`evidence`), sans champ `url` séparé : l'adresse est dans la preuve.

## Règles liées
- BR-FICHE-008, BR-RES-032

## Historique
- 2026-10-17 — création (HRT-122, domaine pur : fiche et chargeur ; le curseur du client et la réponse du catalogue suivent).
