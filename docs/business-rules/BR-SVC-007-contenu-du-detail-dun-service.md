---
id: BR-SVC-007
domaine: SVC
titre: Le détail d'un service porte ses réglages (commande, ports, montages, redémarrage, délai d'arrêt, supervision), jamais une valeur de variable
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-007), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §0, §7, §9, HRT-63
maj: 2026-10-13
---

# BR-SVC-007 — Contenu du détail d'un service

## Règle
Le détail d'un service est le `ServiceItem` du fil, le même pour les deux rôles (BR-SVC-020). Pour un service ADOPTÉ (HRT-79, BR-ADOPT-015), la commande de lancement est réservée aux comptes qui ont le droit « gérer les services » ; aux autres elle est retirée du message et `details_reserved` vaut `true`. Pour un service créé par Hearth rien ne change (BR-ADOPT-016). Contenu : repère d'origine (`origin`, `compose_project`, et, pour un service supervisé, `driver = supervisor` et `unit`), état et depuis quand (`state`, `since`), code de sortie du dernier arrêt (`exit_code`) et relances récentes (`restarts_10m`), date de création, image avec sa version, commande lancée (`command` : le programme puis ses arguments, bornés ; absente quand le moteur n'en annonce aucune), ports, montages, politique de redémarrage (`restart` : `supervisor` pour un service relancé par le système du serveur), délai d'arrêt (`grace_s`) et noms des variables (BR-SVC-008).

- **Délai d'arrêt** : celui de l'UNITÉ (`TimeoutStopUSec`) pour un service supervisé, sinon `Config.StopTimeout` du conteneur, sinon 10 secondes (`stop_grace_s`).
- **Commande** : `Path` puis `Args` de l'inspection, chaque élément nettoyé des caractères de contrôle et plafonné à 1024 caractères, 64 éléments au plus (le dernier est alors « … »). Elle est dans le type `Hidden` côté agent (`Debug` masqué) et `Concealed` sur le fil : jamais dans une trace, un journal, une base ni un message d'erreur. Écart avec la spec : le moteur ne dit pas si la commande est celle de l'image ; l'agent rend ce que le conteneur lance, et le texte « Commande de l'image » n'est dit que quand le moteur n'annonce rien.
- **Service supervisé dont le conteneur n'existe plus** : réglages retenus (nom, image, ports, montages, suppression à l'arrêt, délai) ; ni commande ni noms de variables, qui ne sont jamais retenus.
- **Réseau du serveur** : `host_network = true`, aucun port individuel ; le texte est porté par l'interface.
- **Explication du code de sortie** (0, 137, 143) : portée par l'interface (BR-SVC-023).

## Application (code)
- `crates/hearth-proto/src/api/services.rs::ServiceItem`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::service_item`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::{observation_from, command_of}`
- `crates/hearth-agent/src/domain/services/state.rs::{stop_grace_s, Observation}`
- `crates/hearth-agent/src/domain/services/limits.rs::{MAX_COMMAND_ARGS, MAX_COMMAND_ARG_CHARS}`

## Vérification
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::tests::the_stop_timeout_the_names_and_the_command_are_bounded`
- `crates/hearth-agent/src/domain/services/state.rs::tests::the_stop_grace_is_the_unit_s_then_the_container_s_then_ten_seconds`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::tests::a_supervised_service_names_its_unit_its_driver_and_its_own_grace`
- `crates/hearth-agent/tests/services_https.rs::the_command_and_variable_names_of_an_adopted_service_are_for_the_right_only_the_rest_is_for_all_and_no_value_is_sent`
- `crates/hearth-proto/src/api/services.rs::tests::the_detail_fields_are_absent_until_filled_and_never_show_in_debug`

## Règles liées
- BR-SVC-008, BR-SVC-009, BR-SVC-020, BR-SVC-021

## Historique
- 2026-10-13 — création (HRT-63, tranche 2 des déploiements : détail, superviseur, révélation des variables).
- 2026-10-13 — HRT-79 (ADR-0043) : commande réservée au droit pour un service adopté (BR-ADOPT-015) ; inchangée pour un service créé par Hearth (BR-ADOPT-016).
