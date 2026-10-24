---
id: BR-FICHE-054
domaine: FICHE
titre: Un service adopté ou ordinaire n'a pas d'onglet « Réglages » : seul un service créé par une fiche en a un
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-054), HRT-109
maj: 2026-10-24
---

# BR-FICHE-054

## Règle
L'onglet « Réglages » n'existe que pour un service créé par Hearth depuis une fiche que l'agent propose. Un service adopté (BR-ADOPT-*) ou déployé à la main n'a ni catalogue de réglages ni fichier que Hearth sache lire : l'agent répond `404` à `GET /services/{key}/settings` et le client ne montre pas l'onglet. La liste des services ne dit pas qu'un service est né d'une fiche : le client le demande UNE fois par service à l'agent, garde la réponse (oui ou non) en mémoire et ne montre l'onglet que sur un « oui ». Un compte sans le droit de gérer les services ne voit pas l'onglet (la lecture lui est refusée).

## Application (code)
- `apps/desktop/src/stores/gameSettingsPresence.ts`
- `apps/desktop/src/components/organisms/ServiceDetail.vue` (la liste des onglets)
- `apps/desktop/src-tauri/src/service_ops/game_settings.rs::read` (le « pas de réglages » n'est pas une panne)

## Vérification
- `apps/desktop/src/components/organisms/gameSettingsTab.test.ts::l'onglet`

## Cas limites
- Un agent plus ancien (sans la capacité `game-settings`) répond `404` : même résultat, l'onglet n'apparaît pas.
- Un échec de lecture (lien coupé, agent muet) n'est pas gardé : le client redemandera.

## Règles liées
BR-FICHE-041

## Historique
- 2026-10-24 — création (HRT-109).
