---
id: BR-FICHE-042
domaine: FICHE
titre: Chaque réglage dit ce qui se passe si on le change ; l'effet se déduit du canal, et aucun réglage n'est appliqué à chaud
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-042), contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-035, 036), HRT-108
maj: 2026-10-24
---

# BR-FICHE-042 — L'effet d'un changement se déduit du canal ; aucun réglage n'est appliqué à chaud

## Règle
L'effet d'un réglage n'est jamais déclaré par la fiche : il se déduit du canal. Un réglage du canal `file` demande un redémarrage (« Prend effet au prochain démarrage » : Hearth retouche le fichier, le jeu le relit) ; un réglage des canaux `env` et `args` demande de recréer le service. Un réglage `world_creation` est figé après l'installation. Enregistrer des réglages n'arrête, n'enregistre ni ne redémarre rien : la réponse dit l'effet le plus lourd des changements (`none`, `restart`, `recreate`) et le client lance, s'il le veut, la séquence d'un redémarrage ordinaire (`POST /services/{key}/restart`, BR-JEU-035). Aucun réglage n'est envoyé au jeu pendant qu'il tourne (BR-JEU-036).

L'effet est signé dans l'acte `GameSettingsSave` (`0x1E`) : l'agent le recalcule et refuse (`409 SETTINGS_CONFLICT`, `details.reason = "effect"`) un enregistrement dont l'effet annoncé n'est pas celui qu'il déduit. Tant que la recréation d'un service n'est pas livrée (HRT-87), un changement d'un réglage `env` ou `args` est refusé tout entier avec `409 RECREATE_NOT_AVAILABLE` (`details.ids`), sans rien écrire (deux réglages sur trente et un pour Terraria : `announcement_box_off`, `announcement_box_range`).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::{effect_of, Effect}`
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::save`
- Client (HRT-109) : `apps/desktop/src/settings/messages.ts::effectText` (les quatre mentions), `apps/desktop/src/settings/edits.ts::heaviestEffect`, `apps/desktop/src/components/organisms/GameSettingsTab.vue` (case « Redémarrer maintenant », décochée ; la page ouvre la confirmation de `service_restart`), `apps/desktop/src/stores/gameSettingsPending.ts` (repère « À appliquer », effacé par `composables/useServiceLifecycle.ts` quand un démarrage ou un redémarrage est lancé depuis le client)

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::the_effect_of_a_change_is_deduced_from_the_channel_never_declared`
- `crates/hearth-agent/tests/game_settings_https.rs::a_setting_received_by_argument_asks_for_a_recreation_this_agent_cannot_do`
- `crates/hearth-agent/tests/game_settings_https.rs::the_effect_the_client_signed_must_be_the_one_the_agent_deduces`
- `apps/desktop/src/components/organisms/gameSettingsTab.test.ts::enregistrer (BR-FICHE-042, 048, 053)`

## Cas limites
- Un enregistrement qui ne change aucun octet n'a aucun effet (`none`) et n'écrit rien.
- La case « Redémarrer maintenant », le repère « À appliquer au prochain démarrage » et le texte affiché sont du client (HRT-109) ; l'agent ne garde pas de repère « à appliquer ».

## Règles liées
- BR-FICHE-037, BR-FICHE-048, BR-JEU-035, BR-JEU-036

## Historique
- 2026-10-24 — création (HRT-108, réglages d'un serveur de jeu : lecture, enregistrement, écart, import).
- 2026-10-24 — pointeurs du client (HRT-109).
