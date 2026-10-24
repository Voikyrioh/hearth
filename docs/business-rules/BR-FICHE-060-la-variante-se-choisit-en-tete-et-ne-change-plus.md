---
id: BR-FICHE-060
domaine: FICHE
titre: La variante d'une fiche se choisit en tête de l'étape « Réponses » ; elle ne se change plus après l'installation
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-060), HRT-102
maj: 2026-10-24
---

# BR-FICHE-060

## Règle
Règle portée par le client (HRT-102). Quand la fiche a plusieurs variantes, la première carte de « Réponses » les propose (cartes exclusives jusqu'à quatre, liste au-delà), chacune avec le libellé et l'aide écrits par la fiche ; la variante proposée est présélectionnée et la mention « Ne pourra plus être changée : il faudrait installer un autre serveur. » la suit. Les questions qui ne s'appliquent pas à la variante choisie disparaissent sans message, et leurs réponses ne partent pas. Une fiche à variante unique n'affiche pas la carte. Ce qui ne vaut que pour certaines variantes (question, port, dossier, canal) porte `only_variants` ; « toutes les variantes » s'écrit en OMETTANT le champ, jamais par `only_variants = []` : une liste vide est refusée au chargement de la fiche (motif typé `ListeDeVariantesVide`, invariant de cohérence), pour qu'aucune lecture ne la confonde avec l'absence.

## Application (code)
- `apps/desktop/src/components/molecules/VariantChoice.vue`
- `apps/desktop/src/fiche/answers.ts::visibleQuestions`
- `apps/desktop/src/fiche/ports.ts::forVariant`
- `crates/hearth-agent/src/domain/catalog/loading/loader.rs::check_variant_refs`

## Vérification
- `apps/desktop/src/pages/InstallFromFiche.test.ts::la variante (BR-FICHE-060)`
- `apps/desktop/src/fiche/fiche.test.ts::ne pose que les questions de la variante choisie, et n'envoie que les réponses non vides`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::liste de variantes vide refusée (Motif::ListeDeVariantesVide)`

## Cas limites
- L'agent refuse une réponse d'une autre variante (`SETTING_REFUSED`, raison `variant`).

## Règles liées
BR-FICHE-014, BR-FICHE-018

## Historique
- 2026-10-24 — création (HRT-102).
