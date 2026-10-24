---
id: BR-FICHE-041
domaine: FICHE
titre: L'onglet « Réglages » d'un service de fiche range les réglages par thèmes en mots simples, lus dans la fiche et l'agent
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-041), HRT-109
maj: 2026-10-24
---

# BR-FICHE-041

## Règle
Un service créé par fiche a un onglet « Réglages » (deuxième après « Détail », adresse `?onglet=reglages`) qui montre tous les réglages utiles du jeu, rangés par les thèmes de la fiche dans l'ordre de la fiche. Chaque ligne dit le libellé simple, le nom technique du jeu en petit, la phrase d'explication, le contrôle de son genre, le défaut du jeu (« non vérifié » quand la fiche ne le dit pas), la valeur proposée quand elle diffère, le repère « Modifié », l'une des quatre mentions d'effet (BR-FICHE-042) et « Remettre la valeur proposée ». Les libellés, aides, bornes et choix viennent de la FICHE (`GET /catalog/{id}`, champs `themes` et `settings`) ; les valeurs voulues et l'état du fichier de l'AGENT (`GET /services/{key}/settings`) : le client n'a aucune règle propre à un jeu. Un secret n'est jamais lu : « Défini » ou non. Un réglage figé, imposé, retiré ou reçu par argument (recréation, pas encore livrée) est affiché avec sa raison et un contrôle grisé ; un réglage que le jeu n'a plus va dans « Réglages que le jeu n'a plus ». L'onglet exige le droit de gérer les services ; la capacité `game-settings` de l'agent décide de l'onglet et la lecture n'a lieu qu'à son ouverture (BR-FICHE-054).

## Application (code)
- `apps/desktop/src/components/organisms/GameSettingsTab.vue`, `apps/desktop/src/components/organisms/GameSettingsList.vue`, `apps/desktop/src/components/organisms/SettingThemeCard.vue`, `apps/desktop/src/components/molecules/GameSettingRow.vue`
- `apps/desktop/src/composables/useGameSettings.ts`, `apps/desktop/src/settings/{model,edits,filter,messages}.ts`
- Agent : `crates/hearth-agent/src/application/catalog.rs::setting_view` (catalogue complet servi avec la fiche)

## Vérification
- `apps/desktop/src/components/organisms/gameSettingsTab.test.ts`
- `apps/desktop/src/settings/settings.test.ts`
- `crates/hearth-agent/src/application/catalog.rs::tests::the_detail_serves_the_whole_settings_catalogue_with_its_themes_and_never_a_secret_default`

## Cas limites
- Une fiche d'un agent plus ancien n'a ni thèmes ni réglages : l'onglet le dit (« Ce serveur ne sait pas encore décrire les réglages de cette fiche. »).
- Un réglage ou un thème d'une version plus récente que ce client est compté et dit, jamais montré.

## Règles liées
BR-FICHE-037, BR-FICHE-042, BR-FICHE-043, BR-FICHE-045, BR-FICHE-054

## Historique
- 2026-10-24 — création (HRT-109).
