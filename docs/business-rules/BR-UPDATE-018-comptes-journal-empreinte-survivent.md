---
id: BR-UPDATE-018
domaine: UPDATE
titre: Comptes, journal et empreinte survivent à la mise à jour
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-018), HRT-17
maj: 2026-10-06
---

# BR-UPDATE-018 : Comptes, journal et empreinte survivent à la mise à jour

## Règle
La mise à jour ne touche qu'au binaire : jamais à la base (comptes, sessions, journal), jamais à l'identité (certificat, clé, identifiant d'installation), jamais à la configuration ni à l'unité. Le superviseur vérifie que le nouvel agent présente **le même certificat** (empreinte complète de 32 octets) ; sinon, retour en arrière (BR-UPDATE-015). Les sessions ouvertes restent valables.

## Application (code)
- `crates/hearth-agent/src/application/update_supervisor.rs::Supervisor::{run, ask}` (empreinte dans le travail du superviseur).
- `crates/hearth-agent/src/domain/update/supervise.rs::check_verdict` (`same_identity`).
- Le superviseur n'écrit que dans `update/` et à l'emplacement du binaire.

## Vérification
- `tests/update_supervisor.rs::the_data_beside_the_binary_is_never_touched`, `a_new_agent_with_another_certificate_is_rolled_back_at_once`.
- `deploy/e2e/scenario-update.sh` (empreinte avant et après, nombre de comptes, connexion, journal).

## Interface (HRT-17, lot interface)
- Rien à afficher de plus : la session du client reprend au retour du lien et la liste des comptes et le journal se relisent comme d'habitude. L'écran ne promet rien d'autre que le résultat de la mise à jour.
- Tests : `crates/hearth-link/tests/agent_update.rs::a_restart_announced_by_the_agent_is_an_expected_cut_then_the_result_is_read_back` (le lien revient « Connecté » sur la même session après le redémarrage de l'agent).

## Cas limites
- Une migration de base livrée avec la nouvelle version s'applique au démarrage du nouvel agent ; le retour en arrière remet l'ancien binaire, qui refuse de démarrer sur une base migrée (voir le runbook de mise à jour).

## Règles liées
- ADR-0008 (mises à jour signées), ADR-0012 (service système), ADR-0014 (dépendances de la mise à jour)

## Historique
- 2026-10-05 : création (HRT-17, lot agent, session 2026-10-04-hearth-creation).
- 2026-10-06 : section Interface (HRT-17, lot interface, session 2026-10-04-hearth-creation, T28).
