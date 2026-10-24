---
id: BR-FICHE-057
domaine: FICHE
titre: Chaque chiffre de ressource dit d'où il vient : mesuré par Hearth, mesuré au repos, chiffre de l'éditeur, ou non vérifié
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-057), HRT-102
maj: 2026-10-24
---

# BR-FICHE-057

## Règle
Règle de présentation, portée par le client (HRT-102) à partir de la preuve que l'agent rend avec chaque chiffre (BR-FICHE-008). Le client montre la preuve avec un libellé ET un pictogramme (jamais la couleur seule), propose « Voir la source » pour un chiffre de l'éditeur (adresse relue dans la fiche par la coquille, ouverte dans le navigateur de l'utilisateur) et ne tient pour bloquant qu'un minimum mesuré (BR-FICHE-021). Une preuve que ce client ne connaît pas se lit « non vérifiée ».

## Application (code)
- `apps/desktop/src/fiche/proof.ts`
- `apps/desktop/src/components/molecules/ProofTag.vue`
- `apps/desktop/src-tauri/src/service_ops/catalog.rs::link_of`

## Vérification
- `apps/desktop/src/fiche/fiche.test.ts::la preuve d'un chiffre (BR-FICHE-008)`
- `apps/desktop/src/pages/FicheDetail.test.ts::les ressources et leurs preuves (BR-FICHE-008, 021)`
- `apps/desktop/src-tauri/src/service_ops/catalog.rs::a_link_comes_from_the_fiche_the_agent_sent_never_from_the_page`

## Cas limites
- Hearth ne contacte jamais l'adresse de la source.

## Règles liées
BR-FICHE-008, BR-FICHE-021

## Historique
- 2026-10-24 — création (HRT-102).
