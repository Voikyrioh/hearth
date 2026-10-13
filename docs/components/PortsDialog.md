# PortsDialog

Molécule · `apps/desktop/src/components/molecules/PortsDialog.vue`

Tous les ports d'un service, quand la ligne n'a la place d'en montrer que quelques-uns (« +N »). Élément `<dialog>` natif (piège à focus, Échap), modèle de `AuditDetailDialog` ; le focus revient à la ligne qui l'a ouvert. Ouverte par Entrée sur la ligne ou par un clic sur « +N ».

- Props : `service` (`null` : fermée).
- Événement : `close`.
