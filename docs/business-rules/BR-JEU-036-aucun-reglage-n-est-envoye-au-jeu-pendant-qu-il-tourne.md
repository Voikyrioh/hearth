---
id: BR-JEU-036
domaine: JEU
titre: Aucun réglage n'est envoyé au jeu pendant qu'il tourne ; tout passe par le fichier et un redémarrage, ou par la recréation du service
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-036), HRT-108
maj: 2026-10-24
---

# BR-JEU-036 — Aucun réglage à chaud

## Règle
Hearth n'envoie à un jeu en marche aucune commande de réglage : sans console (BR-JEU-037), il n'a aucun moyen de le faire. Un réglage du canal `file` s'applique par le fichier et un redémarrage ; un réglage des canaux `env` et `args` par la recréation du service, refusée `RECREATE_NOT_AVAILABLE` tant que HRT-87 n'est pas livré (rien n'est écrit, même pour les autres changements de la même demande). Hearth n'envoie au jeu que les textes de la fiche prévus pour l'arrêt et l'enregistrement.

## Application (code)
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::save`
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::effect_of`

## Vérification
- `crates/hearth-agent/tests/game_settings_https.rs::a_setting_received_by_argument_asks_for_a_recreation_this_agent_cannot_do`

## Règles liées
- BR-FICHE-042, BR-JEU-035

## Historique
- 2026-10-24 — création (HRT-108).
