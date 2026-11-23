# ModsTab

Organisme · `apps/desktop/src/components/organisms/ModsTab.vue`

L'onglet « Mods » d'un serveur Minecraft installé par Hearth (HRT-261, HRT-262, BR-MOD-020 à 033, maquettes Main et SansMods) : titre et phrase de bilan, version de Minecraft et sa source, phrases « dernière version à chaque démarrage » et « pack Modrinth », bandeau pour un mod incompatible ACTIVÉ (ou installé deux fois), « Lecture des mods en cours » pour un gros serveur (relecture automatique), la table (`ModRow`), les « Mods retirés », la légende des origines et la phrase de BR-MOD-009. Les moteurs sans mods (Vanilla, plugins) disent leur phrase et proposent « Ajouter un serveur de jeu ». Tenu par `composables/useServiceMods.ts` (lecture, lot de gestes) ; règles pures `services/mods.ts` ; posé par `ServiceDetail` quand la fiche du service offre la voie `drop_jars` (`composables/useServiceDoors.ts`).

- Props : `serverId`, `service`, `gate`, `rightsReasonId`, `connected`
- Événements : `dirty` (un lot non appliqué existe : le détail demande confirmation avant de quitter l'onglet), `addServer`
- Notes : le lot vit dans l'écran, rien n'est écrit avant « Appliquer » ; tests `components/organisms/modsTab.test.ts` et `pages/ServicesMods.test.ts`.
