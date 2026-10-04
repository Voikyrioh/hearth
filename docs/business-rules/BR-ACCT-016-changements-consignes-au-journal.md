---
id: BR-ACCT-016
domaine: ACCT
titre: Tous les changements de compte sont consignés au journal d'activité
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-016), HRT-03
maj: 2026-10-04
---

# BR-ACCT-016 — Tous les changements de compte sont consignés au journal d'activité

## Règle
Création, suppression, changement de rôle, changement de mot de passe et révocation de sessions sont consignés dans le journal d'activité avec l'identité de l'administrateur (ou l'origine « ligne de commande »), l'action et l'horodatage. **Pas encore appliquée** : le journal (table `audit_events`, enregistrement transverse) arrive avec HRT-05. Les cas d'usage de HRT-03 n'écrivent rien au journal.

## Application (code)
- À venir (HRT-05) : couche d'enregistrement transverse et port `AuditSink`. Les cas d'usage de `crates/hearth-agent/src/application/accounts.rs::AccountService` sont les points à instrumenter ; aucune fonction `domain/` ne porte encore cette règle (dette assumée jusqu'à HRT-05).

## Vérification
- À écrire avec HRT-05.

## Cas limites
- Un secret (mot de passe, haché) ne figure jamais dans une ligne de journal.

## Règles liées
- BR-ACCT-015.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
