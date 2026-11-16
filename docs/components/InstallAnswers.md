# InstallAnswers

Organisme · `apps/desktop/src/components/organisms/InstallAnswers.vue`

Les « Réponses » de l'installation (HRT-102, BR-FICHE-014 à 019) : la variante (plusieurs seulement), le nom du service proposé par la fiche, les questions de la fiche (`QuestionField`), le ou les ports (`FichePortField`) et la version de l'image, qui est celle de la fiche. Aucun champ libre pour une variable, une commande, une image ou un dossier. Un défaut de forme n'est montré qu'une fois le champ quitté ou la page validée ; les constats du serveur sont immédiats.

- Props : `install` (`FicheInstallState`, le résultat de `useFicheInstall`)
- Événements et slots : `link` (ouvrir l'adresse où créer un jeton) ; expose `focusQuestion(id)`
- Notes : Tests : `pages/InstallFromFiche.test.ts`.

HRT-109 : `InstallSettings` (« Voir tous les réglages ») est sous la carte des questions.
- Revue UX du 2026-10-24 (constat 16) : expose `focusName()` ; la page y met le curseur à l'arrivée, comme le formulaire de déploiement.
- HRT-244 : « L'essentiel » (la question du monde, les secrets, les questions sans valeur proposée) à l'écran, le reste dans « Réglages avancés », replié (`<details>` piloté, ouvert seul sur un refus ou « Aller au champ », `focusQuestion` async) ; répartition `fiche/questions.ts::splitQuestions` (BR-FICHE-065).
