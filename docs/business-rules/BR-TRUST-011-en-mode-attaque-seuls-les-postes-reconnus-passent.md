---
id: BR-TRUST-011
domaine: TRUST
titre: En mode attaque, seules les connexions des postes reconnus (deux critères sur trois) sont acceptées
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-011) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-011 : En mode attaque, seules les connexions des postes reconnus (deux critères sur trois) sont acceptées

## Règle
Le **mode attaque** est global au serveur (BR-TRUST-010) : il s'applique à tous les comptes, chacun contre ses propres adresses retenues et ses propres clés. Une connexion par mot de passe n'est acceptée que si le poste présente, **avant** le mot de passe, deux critères : l'adresse retenue pour le compte visé (BR-TRUST-007) ET la preuve d'une clé inscrite pour ce compte (BR-TRUST-005). Le mot de passe reste exigé (BR-TRUST-002).

Un seul critère : un essai unique (BR-TRUST-012). Aucun critère : bloqué sans essai (BR-TRUST-016).

Table de vérité (adresse, clé, essai de ce critère déjà utilisé) -> (échappe au ralentissement, mot de passe pris en compte, essai consommé), « du premier coup » ne changeant rien (l'essai en tient lieu) :

| adresse | clé | essai utilisé | échappe | mot de passe pris en compte | essai |
|---|---|---|---|---|---|
| oui | oui | - | oui | oui | aucun |
| oui | non | non | oui | oui | adresse |
| oui | non | oui | non | **non** | aucun |
| non | oui | non | oui | oui | clé |
| non | oui | oui | non | **non** | aucun |
| non | non | - | non | **non** | aucun |

Un mot de passe « non pris en compte » est traité comme faux : même chemin, mêmes compteurs, même réponse (BR-TRUST-017).

## Application (code)
- `crates/hearth-agent/src/domain/trust/recognition.rs::judge_login` (branche `Mode::Attack`, fonction pure).
- `crates/hearth-agent/src/application/sessions.rs::SessionService::verify` : lit la ligne du mode, l'essai du critère présenté seul, appelle `judge_login`, puis `login_policy::conclude`.

## Vérification
- Domaine : `domain::trust::recognition::tests::{the_whole_truth_table_in_attack_mode_is_the_one_written_by_hand, every_combination_of_the_three_modes_is_covered_96_cases}`.
- Intégration : `attack_mode.rs` : `an_administrator_with_key_and_address_always_passes_even_under_attack_and_after_a_typo`, `a_post_without_any_criterion_is_blocked_without_a_trial_even_with_the_right_password`, `without_spoofing_an_attacker_obtains_zero_trial`.

## Cas limites
- Les postes reconnus des **autres** comptes passent aussi (mode global, Q14 point 1) : le libellé « sauf les postes que tu as enregistrés » de la spec est inexact.
- Suspendu (fenêtre de redémarrage, BR-TRUST-020), le mode se comporte comme une ALERTE : cette règle ne s'applique pas.

## Règles liées
- BR-TRUST-001, 002, 012, 016, 017, 034, 035, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
