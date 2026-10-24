---
id: BR-JEU-035
domaine: JEU
titre: Changer un réglage n'arrête, n'enregistre ni ne redémarre rien tout seul ; l'appliquer est la séquence d'un redémarrage ordinaire
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-035), HRT-108
maj: 2026-10-24
---

# BR-JEU-035 — Appliquer des réglages est un redémarrage ordinaire, jamais tout seul

## Règle
Enregistrer des réglages (`PUT /services/{key}/settings`) écrit le fichier du jeu et la valeur voulue, rien de plus : l'agent n'arrête, n'enregistre le monde ni ne redémarre le service. La case « Redémarrer maintenant » est un second geste du client, `POST /services/{key}/restart` (acte `ServiceRestart`, HRT-64), qui déroule la séquence d'arrêt de la fiche (enregistrement du monde avant l'arrêt quand le jeu ne le fait pas lui-même, BR-JEU-002) puis le démarrage : le jeu relit alors son fichier. Un service arrêté prend les réglages à son prochain démarrage lancé depuis Hearth.

## Application (code)
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::save`
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::{request, run_game}`

## Vérification
- `crates/hearth-agent/tests/game_settings_https.rs::saving_touches_only_the_asked_line_and_every_unknown_line_and_comment_survives`
- `crates/hearth-agent/tests/service_lifecycle_https.rs` (la séquence d'un redémarrage ordinaire)
- `crates/hearth-agent/tests/game_stop_https.rs::a_restart_follows_the_same_sequence_then_starts_the_container_again`

## Cas limites
- Cet agent n'a aucune route qui enchaîne l'enregistrement et le redémarrage : l'ordre des deux gestes est celui du client. Le redémarrage d'un serveur de jeu déroule la suite d'arrêt de la fiche (BR-JEU-003), puis démarre le conteneur.

## Règles liées
- BR-FICHE-042, BR-JEU-002, BR-JEU-036

## Historique
- 2026-10-24 — le redémarrage d'un jeu déroule la suite d'arrêt (HRT-103).
- 2026-10-24 — création (HRT-108).
