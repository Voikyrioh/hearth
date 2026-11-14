# DeleteServiceCard

Molécule · `apps/desktop/src/components/molecules/DeleteServiceCard.vue`

La carte « Suppression » du bas du détail d'un service (HRT-86, BR-DEL-002, 003, 026), sur `DashCard`. Un service créé par Hearth montre « Supprimer ce service » (variante `danger`, séparé du pilotage), actif ou désactivé avec sa raison écrite dessous : le droit (décrit par la raison déjà écrite par le détail, une seule fois, BR-GRANT-014), une action en cours (avec « Lancée par {compte}. »), un état que Hearth ne pilote pas, le moteur pas prêt. Un service adopté ne porte aucun bouton : une phrase dit que Hearth ne supprime jamais un service créé hors de Hearth. Un service en déploiement, disparu, ou servi par un agent d'avant `service-delete` n'a pas de carte. Le bouton n'existe nulle part ailleurs (jamais dans le menu de la ligne, BR-DEL-026).

- Props : `door` (`DeleteDoor`, calculée par `services/delete.ts::deleteDoor`), `gate`, `rightsReasonId`
- Événements : `delete`
- Notes : posée par `ServiceDetail`, la page tient la fenêtre. Tests : `pages/ServicesDelete.test.ts`, `services/delete.test.ts`, `e2e/hrt86-delete.spec.ts`.
