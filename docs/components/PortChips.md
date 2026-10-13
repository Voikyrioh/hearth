# PortChips

Molécule · `apps/desktop/src/components/molecules/PortChips.vue`

Au plus trois puces « {port} vers {port} ({protocole}) » puis « +N » (un bouton, hors tabulation : la ligne n'a qu'un arrêt), ou « Aucun port ». Un service sur le réseau du serveur dit « Utilise directement le réseau du serveur : aucun port individuel. » (BR-SVC-014).

- Props : `ports`, `omitted`, `hostNetwork`, `width` (colonne de 270 px en large, 220 en compact).
- Événement : `more` (clic sur « +N » : la fenêtre `PortsDialog`).
