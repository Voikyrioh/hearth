# ProofTag

Molécule · `apps/desktop/src/components/molecules/ProofTag.vue`

D'où vient un chiffre de ressource d'une fiche (HRT-102, BR-FICHE-008, BR-FICHE-057) : « Mesuré par Hearth », « Mesuré au repos », « Chiffre de l'éditeur » ou « Non vérifié ». Le sens est dans le texte et dans un pictogramme propre à chaque preuve, jamais dans la couleur seule. Pour un chiffre de l'éditeur, « Voir la source » peut s'ajouter : un bouton qui émet `source` (la page demande à la coquille d'ouvrir l'adresse de la fiche dans le navigateur de l'utilisateur ; Hearth ne la contacte jamais).

- Props : `proof` (`measured`, `measured_idle`, `publisher`, `unmeasured`), `source` (propose « Voir la source »), `subject` (ce que le chiffre mesure, pour nommer le lien)
- Événements et slots : `source`
- Notes : Règles pures : `fiche/proof.ts`. Tests : `pages/FicheDetail.test.ts`, `fiche/fiche.test.ts`.
