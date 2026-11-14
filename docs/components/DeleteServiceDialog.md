# DeleteServiceDialog

Organisme · `apps/desktop/src/components/organisms/DeleteServiceDialog.vue`

La fenêtre « Supprimer {nom} ? » (HRT-86, BR-DEL-004 à 013, 022) : une fenêtre pour l'effet et la confirmation du socle (`AdminActDialog`, acte `service_delete`, jamais couvert par l'élévation : le mot de passe est demandé à chaque fois, même sans case cochée). Elle lit d'abord la prévisualisation des dossiers (mesure de 10 secondes au plus), dit l'effet (service retiré, configuration effacée, journaux perdus, image conservée), annonce l'arrêt d'un service en marche avec son délai réel, puis montre le tableau « Dossier · Taille · Éléments · Sort » (chemins complets en police à chasse fixe, « plus de {taille} (mesure interrompue) », dossiers partagés, non vérifiables ou du serveur toujours conservés) et deux cases DÉCOCHÉES : « Supprimer aussi les données » (seulement s'il y a un dossier effaçable) et « Supprimer aussi les sauvegardes de ce service » (seulement si Hearth en connaît). Cocher une case demande de retaper le nom, casse comprise. Si le service change sous la fenêtre (disparu, état, action en cours, moteur) elle le dit et la confirmation devient inactive ; « Annuler » ne laisse aucune trace. « Supprimer le service » est plein rouge.

- Props : `open`, `serverId`, `service`, `current`, `locked`, `engineReady`, `preview` (`PreviewState`), `perform`
- Événements : `close`, `retry`
- Notes : posée par `pages/Services.vue` avec `composables/useServiceDelete.ts`. Tests : `pages/ServicesDelete.test.ts`, `e2e/hrt86-delete.spec.ts`.
