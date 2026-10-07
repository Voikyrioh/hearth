---
id: BR-TRUST-047
domaine: TRUST
titre: Aucune décision d'un acte d'administration ne dépend de l'horloge murale ; la garde de réactivation du mode attaque est bornée par le temps de marche depuis le démarrage
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md (sections 4.6 et 5, tranches D1 et E) ; contexts/hearth/tickets/hrt/HRT-28.md (suivi r2 de la PR #28) ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-047 : Aucune décision d'un acte ne lit l'horloge murale

## Règle
- Le défi d'un acte (60 s) et l'élévation du mot de passe (5 minutes, BR-TRUST-043) se mesurent sur l'horloge monotone de l'agent : reculer ou avancer l'heure ne prolonge ni ne raccourcit rien.
- **Garde de réactivation du mode attaque** (BR-TRUST-012, Q14 point 8) : une activation faite moins de 30 minutes après la fin de la précédente la prolonge (même `activation_id`, essais non rendus). Quand la machine a redémarré depuis cette fin (identifiant de démarrage lisible et différent de celui de la fin), la garde ne tient plus que `REARM` (30 minutes) **de marche** : la fin a eu lieu avant ce démarrage, donc le temps écoulé depuis elle est au moins le temps de marche. Avant ces 30 minutes, l'horloge murale ne peut que rallonger la garde (une fin « dans le futur » reste récente) ; après, une heure reculée d'un an ne la tient plus. Identifiant de démarrage illisible : horloge murale seule, comme avant. Même démarrage : le temps de marche seul décide, inchangé.

## Application (code)
- `crates/hearth-agent/src/domain/trust/attack_mode.rs::plan_activation`.

## Vérification
- `domain::trust::attack_mode::tests::a_rebooted_machine_with_the_clock_set_back_keeps_the_guard_for_thirty_minutes_of_uptime_only`.
- `tests/admin_reauth.rs::setting_the_wall_clock_back_or_forward_never_moves_the_five_minutes`.
- `tests/attack_mode.rs::a_rebooted_machine_with_the_clock_set_back_a_year_drops_the_guard_after_thirty_minutes_of_uptime` ; les tests de la garde sur le même démarrage sont inchangés.

## Règles liées
- BR-TRUST-012, BR-TRUST-019, BR-TRUST-020, BR-TRUST-043, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-28, tranche E).
- 2026-10-07 : HRT-28, tranche D1 : l'élévation du mot de passe entre dans la règle.
