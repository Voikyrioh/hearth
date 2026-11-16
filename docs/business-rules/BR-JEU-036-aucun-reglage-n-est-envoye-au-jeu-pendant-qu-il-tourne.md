---
id: BR-JEU-036
domaine: JEU
titre: Aucun réglage n'est envoyé au jeu pendant qu'il tourne ; tout passe par le fichier et un redémarrage, ou par la recréation du service
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-036), HRT-108
maj: 2026-11-16
---

# BR-JEU-036 — Aucun réglage à chaud

## Règle
Hearth n'envoie à un jeu en marche aucune commande de réglage : sans console (BR-JEU-037), il n'a aucun moyen de le faire. Un réglage du canal `file` s'applique par le fichier et un redémarrage ; un réglage des canaux `env` et `args` par la recréation du service (HRT-266) : les valeurs voulues et la description sont enregistrées, la fiche est traduite de nouveau avant et après et seule la différence des variables et de la commande est appliquée (jamais un secret tiré à l'installation, jamais un client qui dicte une variable). Si la recréation est refusée (`RECREATE_NOT_AVAILABLE` : moteur pas prêt, état, port), les valeurs des réglages `env` et `args` reviennent à ce qu'elles étaient. Hearth n'envoie au jeu que les textes de la fiche prévus pour l'arrêt et l'enregistrement.

## Application (code)
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::save`
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::effect_of`

## Vérification
- `crates/hearth-agent/tests/game_settings_https.rs::a_setting_received_by_argument_recreates_the_service_with_a_new_revision`

## Règles liées
- BR-FICHE-042, BR-JEU-035

## Historique
- 2026-11-17 : une demande qui mêle fichier et conteneur est refusée en entier AVANT toute écriture quand la recréation n'est pas possible (`ServiceUpdates::precheck`) ; un refus tardif remet l'état et les scellés des réglages du conteneur (HRT-266).
- 2026-11-16 : la recréation est livrée (HRT-266).
- 2026-10-24 — création (HRT-108).
