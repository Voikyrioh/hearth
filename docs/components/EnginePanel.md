# EnginePanel

Organisme · `apps/desktop/src/components/organisms/EnginePanel.vue`

Carte « Moteur de conteneurs » : les huit états du moteur et « Vérification du moteur… » par un mot ET une icône, nom, version et compteurs à « Prêt », explication et aide à recopier (`CommandBlock`, administrateur seulement) pour chaque état non prêt, « Vérifier à nouveau ». Aucun bouton d'installation ni de démarrage.

- Props : `engine` (`null` : première détection), `counts`, `isAdmin`, `previous`, `recheck`. Événements : `recheck`, `docs`. Aide : `services/help.ts`.
