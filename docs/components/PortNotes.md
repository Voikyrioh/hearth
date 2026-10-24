# PortNotes

Molécule · `apps/desktop/src/components/molecules/PortNotes.vue`

Les notes sous un port du serveur (HRT-69, HRT-102, BR-DEPLOY-007 à 009) : l'avertissement de l'agent (un port réservé par un service arrêté se confirme au lancement), le port sous 1024 et le port libre le plus proche que l'agent a calculé quand le port saisi est refusé (« Utiliser le port … »). Aucun conflit n'est recalculé ici ; les avertissements ne se montrent qu'en l'absence d'erreur.

- Props : `verdict` (`PortVerdict`), `privileged`, `clean` (aucune erreur sous le champ)
- Événements et slots : `suggest` (le port proposé)
- Notes : partagé par `PortMappingRow` (déploiement d'une image) et `FichePortField` (installation d'une fiche) ; les textes sont les clés `deploy.*`. Tests : `pages/DeployService.test.ts`, `pages/InstallFromFiche.test.ts`.
- Revue UX du 2026-10-24 (constat 12) : « Utiliser le port … » est un bouton secondaire posé sous l'encart (et non dans l'encart teinté, où son bord disparaissait), comme « Ajouter un port ».
