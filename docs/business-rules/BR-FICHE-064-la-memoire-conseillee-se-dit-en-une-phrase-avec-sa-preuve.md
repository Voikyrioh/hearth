---
id: BR-FICHE-064
domaine: FICHE
titre: La mémoire conseillée d'une fiche se dit en une phrase qui nomme sa preuve, jamais « tient » ou « ne tient pas »
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-choisir-un-jeu-dans-le-catalogue.md (BR-FICHE-064), HRT-237, HRT-241
maj: 2026-11-16
---

# BR-FICHE-064 — La mémoire conseillée en une phrase

## Règle
**Donnée (HRT-237).** Chaque carte de `GET /catalog` porte `memory` : `{ recommended_bytes, evidence }`, la mémoire RECOMMANDÉE de la fiche et la preuve de ce chiffre (BR-FICHE-057 : mesuré, mesuré au repos, éditeur avec son adresse, non vérifié). Le membre est absent quand la fiche n'a pas de mémoire recommandée ; il est facultatif pour un client ancien. Les ressources d'une fiche ne dépendent pas de la variante (aucune variante ne porte de ressources) : la mémoire de la carte est celle de la fiche, donc de sa variante par défaut comme des autres.

**Phrase (HRT-241).** La page dit ce chiffre en une phrase qui nomme d'où il vient (mesuré, mesuré sans joueur, chiffre de l'éditeur, non vérifié) et, si elle est connue, la mémoire de la machine. Elle ne dit jamais qu'un jeu « tient » ou « ne tient pas » sur la carte : ce jugement reste à l'étape « Mémoire et processeur ».

## Application (code)
- `crates/hearth-agent/src/application/catalog.rs::memory_hint`
- `crates/hearth-proto/src/api/catalog.rs::{MemoryHint, FicheCard}`
- `apps/desktop/src/fiche/memory.ts::memorySentence` (HRT-241)

## Vérification
- `crates/hearth-agent/src/application/catalog.rs::the_card_carries_the_recommended_memory_with_its_proof_and_none_without_a_figure`
- `crates/hearth-agent/tests/catalog_https.rs::the_list_carries_each_recommended_memory_with_its_evidence`

## Cas limites
- Chiffre de l'éditeur : la phrase ne cite jamais la machine (le chiffre n'est pas mesuré par Hearth).
- Preuve inconnue d'une version plus récente : lue « non vérifiée ».

## Règles liées
- BR-FICHE-003, BR-FICHE-008, BR-FICHE-057

## Historique
- 2026-11-16 — création (HRT-237, partie donnée).
