# ServiceRightReason

Molécule · `apps/desktop/src/components/molecules/ServiceRightReason.vue`

La raison écrite SOUS un bouton d'action de service désactivé faute du droit « gérer les services » (HRT-78, BR-GRANT-014). Un seul composant pour tous les boutons de service. Administrateur sans le droit : « Ton compte ne peut pas gérer les services de ce serveur. », la phrase d'introduction et la commande à copier ; lecture seule : la raison et « Demande à la personne qui administre ce serveur. » ; agent ancien : « Ce serveur doit être mis à jour pour gérer les droits sur les services. » ; lecture en échec : « Impossible de lire ton droit sur les services. Réessaie. » et « Vérifier à nouveau ». Rien quand la lecture est en cours, que le droit est accordé ou que le pilotage est désactivé.

- Props : `gate` (`RightsGate`, `services/rights.ts`), `id` (celui du texte décrit)
- Événements : `retry`
- Notes : le bouton expliqué porte `HButton describedBy = id` (lu avec lui, jamais seulement en infobulle) ; la commande n'est pas dans le texte décrit. Aucune case, aucun commutateur (BR-GRANT-005). Livré avec `useServiceRights`, branché pour l'instant seulement dans un bandeau de la page Services (aucun bouton d'action de service n'existe encore). Tests : `serviceRights.test.ts`, `pages/ServicesRights.test.ts`, `e2e/services-rights.spec.ts`.
