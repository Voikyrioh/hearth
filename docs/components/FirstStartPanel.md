# FirstStartPanel

Organisme · `apps/desktop/src/components/organisms/FirstStartPanel.vue`

Le premier démarrage long d'un serveur de jeu (HRT-102, BR-FICHE-056) : un encart d'information qui ne dit que des faits (« Premier démarrage : le jeu prépare ses fichiers. Ce n'est pas une panne. », l'étape que la fiche sait reconnaître, les octets déjà écrits et le total attendu quand la fiche le connaît avec une preuve). Jamais de barre, de pourcentage, de durée restante ni de compteur qui défile. Durée annoncée dépassée : « Hearth ne peut pas savoir si le jeu est prêt. »

- Props : `firstStart` (`FicheFirstStart`), `facts` (`{ stage, writtenBytes, elapsedSeconds }`)
- Événements et slots : —
- Notes : Pas encore posé dans le détail d'un service : l'agent ne projette pas la phase ni le relevé (suivi HRT-197). Règles pures : `fiche/firstStart.ts`. Tests : `fiche/fiche.test.ts`.
