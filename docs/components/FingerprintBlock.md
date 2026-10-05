# FingerprintBlock

Molécule · `apps/desktop/src/components/molecules/FingerprintBlock.vue`

Empreinte d'un serveur : 8 groupes de 4 caractères en DM Mono 20 px, sur 2 lignes de 4, dans un cadre ; `tone="crit"` borde l'empreinte reçue de l'alerte d'identité changée. BR-CONN-001.

- Props : `value` (texte en 8 groupes séparés par des espaces), `label`, `tone`
- Événements et slots : aucun
- Notes : Utilisée par l'assistant, le formulaire de modification et l'alerte. Tests : `connect.test.ts`, `Servers.test.ts`.
