---
id: BR-INSTALL-007
domaine: INSTALL
titre: Une réinstallation à la même version n'interrompt pas le service inutilement
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-agent.md (BR-INSTALL-007), HRT-15
maj: 2026-10-05
---

# BR-INSTALL-007 : Une réinstallation à la même version n'interrompt pas le service inutilement

## Règle
Si l'installation est complète, que le binaire installé a exactement les mêmes octets que celui qu'on installe et que le service tourne, rien n'est touché : ni binaire, ni unité, ni redémarrage (« L'agent est déjà à jour. Le service n'a pas été interrompu. »). Si les octets diffèrent (reconstruction) ou si le service est arrêté, il est redémarré (« L'agent est déjà à jour. Service redémarré. »).

## Application (code)
- `crates/hearth-agent/src/domain/install/plan.rs::plan_install` (`service_action`, `replace_binary`) : `ServiceAction::Leave`.
- `crates/hearth-agent/src/infrastructure/install/host.rs::same_bytes` : comparaison des octets, jamais de la seule taille.

## Vérification
- `domain::install::plan::tests::the_same_binary_on_a_running_service_does_not_interrupt_it`.
- `tests/install_flow.rs::the_same_binary_on_a_running_service_does_not_interrupt_it`.

## Cas limites
- La spécification donne « Service redémarré » pour la réinstallation à même version ; il reste le message d'un service effectivement redémarré. « Le service n'a pas été interrompu » est un ajout (HRT-15).

## Règles liées
- BR-INSTALL-003

## Historique
- 2026-10-05 : création (HRT-15, session 2026-10-04-hearth-creation).
