---
id: FIX-01M4ECZJBH94ABZXSTNNFFGZ5P
titre: Un bouton grisé n'expliquait sa raison qu'au survol (C42)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4ECZJBH94ABZXSTNNFFGZ5P : Un bouton grisé n'expliquait sa raison qu'au survol (C42)

## Symptôme
« Mettre à jour l'agent » (lecture seule), le mode attaque, « Changer mon mot de passe » et « Supprimer mon compte » hors ligne ou sans le rôle : la raison n'apparaissait qu'au survol.

## Reproduction
`e2e/hrtx.spec.ts` « la raison d'un bouton grisé est écrite sous le bouton » aux 5 tailles. Rouge avant : aucun texte sous le bouton.

## Cause root
`HButton reason-below` existait mais n'était pas adopté.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
`reason-below` sur le bouton de l'agent, le basculement du mode attaque et les deux boutons de « Mon compte ». Non adoptés, par choix : les boutons des lignes de tableau (comptes, postes de confiance : une raison par ligne noierait le tableau), les boutons de l'en-tête de page (Ajouter un compte, Exporter) et les actions des bandeaux de sécurité (même raison écrite sur la page Sécurité). `// FIX:01M4ECZJBH94ABZXSTNNFFGZ5P`.

## Règles
- Aucune règle métier modifiée.

## Non-régression
- Les tests ci-dessus.

## Références
- Tickets : HRT-39, HRT-40, HRT-41, HRT-43 (revue UX du 2026-10-08)
- Code : `components/organisms/AgentUpdateCard.vue, AttackModePanel.vue, OwnAccountCard.vue`
