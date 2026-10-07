---
id: BR-UPDATE-034
domaine: UPDATE
titre: Une machine redémarrée pendant une mise à jour : l'agent qui démarre conclut d'après le marqueur et les faits
statut: active
invariant: true
source: HRT-27, ADR-0014 (amendement du 2026-10-07)
maj: 2026-10-07
---

# BR-UPDATE-034 : Une machine redémarrée pendant une mise à jour : l'agent qui démarre conclut d'après le marqueur et les faits

## Règle
L'unité transitoire du superviseur n'existe plus après un redémarrage de la machine : plus personne ne relance le superviseur, mais le marqueur d'étape (BR-UPDATE-032) et les traces restent sur le disque. Au démarrage, l'agent applique BR-UPDATE-028 (`classify_orphan`, d'après la version qui tourne) :
- **le binaire en place est encore l'ancien** (arrêt avant l'échange) : la tentative est conclue `failed` / `interrupted`, dépôt, copie et marqueur retirés ; l'ancien agent tourne sur sa base ;
- **le binaire en place est le nouveau, sauvegarde gardée** (échange fait, rien de conclu) : l'agent lance un superviseur de reprise (`Job::recover`), qui **reprend le marqueur là où il s'est arrêté** : à `swapped` ou `checking`, il contrôle le nouvel agent (réussite, ou retour arrière) ; à `rolling_back`, `rollback_stopped` ou `rollback_database` (retour arrière interrompu), il le **continue** : il arrête le service, remet la base d'avant une seconde fois (le nouveau binaire, démarré avec la machine, a pu la migrer de nouveau) puis l'ancien binaire ; à `rollback_binary`, la base ne se remet plus ;
- **l'ancien binaire est déjà revenu** : conclu sans toucher à la base (BR-UPDATE-028) ;
- **trace illisible** : conclu sans deviner, copies gardées (BR-UPDATE-028, BR-UPDATE-032).

Après le redémarrage, l'agent ne cède plus à systemd (l'unité transitoire n'existe plus : `supervisor_pending` est faux) ; « reprise déjà tentée » se lit dans le marqueur (reprises sous la borne : une reprise est relancée ; à la borne : `RecoveryAlreadyTried`).

**Limite assumée** : si le nouveau binaire **ne démarre pas du tout** au redémarrage de la machine (l'échange est fait, l'ancien binaire n'est pas revenu), aucun agent ne tourne pour conclure et aucune unité n'existe plus pour relancer le superviseur : reprise à la main (runbook). Y remédier demande une unité PERSISTANTE écrite par l'installation, ce que HRT-27 a écarté (ADR-0014) ; suivi proposé dans le ticket.

## Application (code)
- `crates/hearth-agent/src/domain/update/orphan.rs::classify_orphan` (inchangée) ; `domain/update/resume.rs::resume_phase` ; `application/update.rs::UpdateService::{resume, recover}`.

## Vérification
- `tests/update_supervisor_resume.rs::a_machine_rebooted_at_any_point_of_an_update_ends_with_a_coherent_agent` (réussite et retour arrière, mort à chaque point, redémarrage, démarrage de l'agent : jamais un ancien binaire devant une base migrée, jamais un retour arrière rejoué, un couple cohérent).
- `domain::update::resume::tests::a_rollback_interrupted_after_its_stop_resumes_at_its_stop_to_put_the_database_back_again`.

## Règles liées
- BR-UPDATE-028, BR-UPDATE-029, BR-UPDATE-032, ADR-0014

## Historique
- 2026-10-07 : création (HRT-27).
