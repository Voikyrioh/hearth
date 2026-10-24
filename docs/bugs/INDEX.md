# Bugs — Hearth

Fiches d'anomalies découvertes et corrigées : marquage `// FIX:{ULID}` dans le code, fiche explicative `FIX-{ULID}.md` documentant symptôme, cause root, workaround, correction.

| Fiche | Titre | Date |
|---|---|---|
| [FIX-01M5PYVP4V48YGH36AYS43Q31V](./FIX-01M5PYVP4V48YGH36AYS43Q31V.md) | Le message d'un caractère refusé s'arrêtait sur deux points quand le caractère était une espace (HRT-102) | 2026-10-24 |
| [FIX-01M5PYVP4WFPG3YMVV68T041R4](./FIX-01M5PYVP4WFPG3YMVV68T041R4.md) | Le récapitulatif « Vérifiez avant de lancer » s'ouvrait défilé tout en bas (HRT-69, HRT-102) | 2026-10-24 |
| [FIX-01M5PYVP4WVBS6H4Q6M1QW4DQG](./FIX-01M5PYVP4WVBS6H4Q6M1QW4DQG.md) | Le simulateur donnait deux machines : 16 cœurs et 64 Go au tableau de bord, 4 cœurs et 16 Go aux curseurs et à la fiche (HRT-69) | 2026-10-24 |
| [FIX-01M5P0Q3T8ZK6V2R9XH4DWB7NC](./FIX-01M5P0Q3T8ZK6V2R9XH4DWB7NC.md) | L'action suivante d'un service était refusée (409 SERVICE_BUSY) juste après avoir lu l'issue terminale de la précédente (HRT-101) | 2026-10-24 |
| [FIX-01M5P0Q7C3N8T5W2R9XK4D6HJE](./FIX-01M5P0Q7C3N8T5W2R9XK4D6HJE.md) | Le test de mise à jour de l'agent tombait au hasard quand `systemctl kill` sortait en erreur sur le cgroup (HRT-206) | 2026-10-24 |
| [FIX-01M5NYA5W781YW5NFQ7W1JGR98](./FIX-01M5NYA5W781YW5NFQ7W1JGR98.md) | Une opération de déploiement ou de cycle de vie était annoncée terminée avant l'écriture de son entrée au journal d'activité (HRT-101) | 2026-10-24 |
| [FIX-01M5A5VG2H1ESNN212NNMT2PP0](./FIX-01M5A5VG2H1ESNN212NNMT2PP0.md) | Le client proposait Arrêter, Redémarrer et Démarrer sur un service créé hors de Hearth et non adopté, puis répondait « Ce service n'existe plus. » (HRT-65) | 2026-10-19 |
| [FIX-01M5A5VH5GR2M9PF9Y1SB04D65](./FIX-01M5A5VH5GR2M9PF9Y1SB04D65.md) | L'action lancée par le compte connecté s'affichait « déjà en cours, lancée par marie » (HRT-65) | 2026-10-19 |
| [FIX-01M5A5VJ8G6Q6FDJV9AX6ECF41](./FIX-01M5A5VJ8G6Q6FDJV9AX6ECF41.md) | Un cadre de focus blanc entourait le titre du détail après un clic ou une action (HRT-65) | 2026-10-19 |
| [FIX-01M5A5VKBEZ7XCZ2P0CD2VXV9M](./FIX-01M5A5VKBEZ7XCZ2P0CD2VXV9M.md) | L'étiquette « toi » de l'écran Comptes tutoyait l'utilisateur (HRT-65) | 2026-10-19 |
| [FIX-01M5A5VMDW33WJG3S1BWKE3N15](./FIX-01M5A5VMDW33WJG3S1BWKE3N15.md) | Avertissement Vue « attribut sans destinataire » à chaque ouverture d'une confirmation de pilotage (HRT-65) | 2026-10-19 |
| [FIX-01M52VPMJ3KKEN1MP5G1C63QQN](./FIX-01M52VPMJ3KKEN1MP5G1C63QQN.md) | La preuve d'absence d'un objet refusait la ligne d'erreur préfixée `aws: [ERROR]: ` des AWS CLI récents (HRT-171) | 2026-10-16 |
| [FIX-01M4VNGP6RMTHA7XNDDGV2FH1R](./FIX-01M4VNGP6RMTHA7XNDDGV2FH1R.md) | L'état « à confirmer » d'une adoption ne survivait pas au redémarrage, et le passage de relecture jugeait « non relue » une adoption faite pendant ses lectures (HRT-80, PR 89) | 2026-10-14 |
| [FIX-01M4VNGP6R446RGM9FB3JEJG7P](./FIX-01M4VNGP6R446RGM9FB3JEJG7P.md) | Après un échec d'écriture, la lecture des candidats du même compte était servie sans entrée au journal (HRT-80, PR 89) | 2026-10-14 |
| [FIX-01M4W5A9CK7Q2R8T3X6V0DNYHE](./FIX-01M4W5A9CK7Q2R8T3X6V0DNYHE.md) | `decode_segment` acceptait `%+f` (la couche et le routeur ne lisaient pas la même clé) (HRT-80) | 2026-10-14 |
| [FIX-01M4VR6Y4Q4AVQVT9FCME7Z2SB](./FIX-01M4VR6Y4Q4AVQVT9FCME7Z2SB.md) | Une étape, une issue ou un code inconnu d'un agent plus récent faisait rejeter ou perdre le message entier (BR-COMPAT-001) | 2026-10-14 |
| [FIX-01M4V4G0ZPWMKE6WCYKPF2Q3SM](./FIX-01M4V4G0ZPWMKE6WCYKPF2Q3SM.md) | La liste des dossiers refusés au montage laissait passer /usr, /nix, /run, /var/spool et le .ssh d'un compte (HRT-75) | 2026-10-13 |
| [FIX-01M4SFCSXW2P3QXP6FY8CS6ZMQ](./FIX-01M4SFCSXW2P3QXP6FY8CS6ZMQ.md) | Textes de conteneur : séparateurs de ligne Unicode non retirés, ports et montages omis non comptés (HRT-72) | 2026-10-13 |
| [FIX-01M4SFCRJWX8KMBC1ZK9ZWETTW](./FIX-01M4SFCRJWX8KMBC1ZK9ZWETTW.md) | La date de la liste sur le fil faisait surestimer son âge, sans règle pour l'écart d'horloge (HRT-72, N2) | 2026-10-13 |
| [FIX-01M4SFCQ7N5TN8VC21KBQR87PP](./FIX-01M4SFCQ7N5TN8VC21KBQR87PP.md) | Au retour du moteur, l'ancienne liste repartait sous « prêt » sans marque (HRT-72, N1) | 2026-10-13 |
| [FIX-01M4EPVMJGC21SAKM7YBP4S7S9](./FIX-01M4EPVMJGC21SAKM7YBP4S7S9.md) | L'installateur proposait un ancien dossier qui n'existait plus (S6) | 2026-10-08 |
| [FIX-01M4EPVM3MK6PDHQDYKQGQB09R](./FIX-01M4EPVM3MK6PDHQDYKQGQB09R.md) | Un volume overlay de Docker rendu comme un disque (S1a) | 2026-10-08 |
| [FIX-01M4EPX90N9QNNPCHVNB6MCDDF](./FIX-01M4EPX90N9QNNPCHVNB6MCDDF.md) | Ajout d'un serveur : les éléments n'étaient pas alignés pareil à gauche (anneau de couleur, halo de focus, chiffres des étapes) (S3) | 2026-10-09 |
| [FIX-01M4EPX88BTXFX1PX7E57WGK38](./FIX-01M4EPX88BTXFX1PX7E57WGK38.md) | Un point de montage ou un nom de sonde long sortait de sa carte, un nom court s'écrivait en colonne (S1b) | 2026-10-08 |
| [FIX-01M4EPX8PWN1RRX8A4HZGTKF0E](./FIX-01M4EPX8PWN1RRX8A4HZGTKF0E.md) | L'anneau du serveur sélectionné était rogné dans la barre des serveurs (S2) | 2026-10-08 |
| [FIX-01M4EPX99456BPK54R5JHE4JPA](./FIX-01M4EPX99456BPK54R5JHE4JPA.md) | Les textes d'exemple de l'ajout d'un serveur étaient copiés de la machine de Voiky (S4) | 2026-10-08 |
| [FIX-01M4EHYN77QBS7F7T5FCTRH6R5](./FIX-01M4EHYN77QBS7F7T5FCTRH6R5.md) | Journal à 1280 : le tableau dépassait sa carte, deux barres horizontales empilées, « Raison » coupée (D1) | 2026-10-08 |
| [FIX-01M4EHYNER6SJ3ZZVH286TD3FB](./FIX-01M4EHYNER6SJ3ZZVH286TD3FB.md) | Journal : en-têtes décalés de 4 à 10 px de leurs colonnes avec une barre de défilement classique (C31) | 2026-10-08 |
| [FIX-01M4EHYNPBB8TZ89DRRZ7TRH0A](./FIX-01M4EHYNPBB8TZ89DRRZ7TRH0A.md) | Les bandeaux de sécurité étaient bornés à 880 px au-dessus d'une page plus large, boutons empilés contre les bords (D2) | 2026-10-08 |
| [FIX-01M4EHYNXY72JB4KW0994J85AB](./FIX-01M4EHYNXY72JB4KW0994J85AB.md) | Comptes : l'en-tête de page débordait du tableau borné à 1 400 px (D3) | 2026-10-08 |
| [FIX-01M4EHYP5JP846FGWCXJ41ET32](./FIX-01M4EHYP5JP846FGWCXJ41ET32.md) | À 1100, les légendes des courbes passaient sur deux lignes (D4) | 2026-10-08 |
| [FIX-01M4EHYPD658X22RH4SFV0TDVE](./FIX-01M4EHYPD658X22RH4SFV0TDVE.md) | Comptes : Échap sur « Changer le rôle » laissait le curseur nulle part (D6) | 2026-10-08 |
| [FIX-01M4EHYPMVHR2TY39G3BQ7ZJZA](./FIX-01M4EHYPMVHR2TY39G3BQ7ZJZA.md) | « Mes serveurs » : la pastille d'état passait seule à la ligne (D8) | 2026-10-08 |
| [FIX-01M4EDC0FVT0ZHGASFEMWX7066](./FIX-01M4EDC0FVT0ZHGASFEMWX7066.md) | Réglages et Sécurité : une colonne étroite, une colonne étirée, et du contenu sans borne à 1920 et 2560 (C44, C55 en partie) | 2026-10-08 |
| [FIX-01M4ECZJ34NZE9Z9P4YATK707H](./FIX-01M4ECZJ34NZE9Z9P4YATK707H.md) | Une courbe qui a le focus récitait une valeur par seconde (suivi de la revue de #58) | 2026-10-08 |
| [FIX-01M4ECZJBH94ABZXSTNNFFGZ5P](./FIX-01M4ECZJBH94ABZXSTNNFFGZ5P.md) | Un bouton grisé n'expliquait sa raison qu'au survol (C42) | 2026-10-08 |
| [FIX-01M4ECZJKG6MQ2C65TZP25WSPD](./FIX-01M4ECZJKG6MQ2C65TZP25WSPD.md) | La fenêtre de retrait d'un poste alignait quatre paragraphes avant le champ (C39) | 2026-10-08 |
| [FIX-01M4ECZJVHHJA1MNQ2E4FSFJ0K](./FIX-01M4ECZJVHHJA1MNQ2E4FSFJ0K.md) | « Un poste à moitié reconnu » : jargon sur la page Sécurité (C34, reste) | 2026-10-08 |
| [FIX-01M4ECZK55R0HTKS7SQH4WZ1ZZ](./FIX-01M4ECZK55R0HTKS7SQH4WZ1ZZ.md) | Après un échec de lecture, « Réessayer » relançait l'ancien filtre, et rien ne disait que la recherche tapée n'était pas appliquée (HRT-43) | 2026-10-08 |
| [FIX-01M4E9T718D37EXTXMWA7YXWJE](./FIX-01M4E9T718D37EXTXMWA7YXWJE.md) | Une courbe du tableau de bord ne disait ni sa durée, ni son échelle, ni sa valeur (C10) | 2026-10-08 |
| [FIX-01M4E9T77SYNC7MK8Q1JP5PQ4J](./FIX-01M4E9T77SYNC7MK8Q1JP5PQ4J.md) | Les jauges à l'état normal étaient dessinées en dégradé rose-braise, la couleur d'une alerte (C13) | 2026-10-08 |
| [FIX-01M4E9T7ECW7H6R9V4V6YNVRE1](./FIX-01M4E9T7ECW7H6R9V4V6YNVRE1.md) | Tableau de bord sans mesure : « Vu il y a… » posé sur une page qui n'a rien vu | 2026-10-08 |
| [FIX-01M4E82YHBNR94DXKJATXCQ5TM](./FIX-01M4E82YHBNR94DXKJATXCQ5TM.md) | Journal jamais chargé hors ligne : un cadre vide sous une estampille (C46) | 2026-10-08 |
| [FIX-01M4E5D3N2QCVCD494WC6S8WRD](./FIX-01M4E5D3N2QCVCD494WC6S8WRD.md) | Action impossible : la fenêtre garde sa question et un bouton grisé, sans chemin (C40, C41) | 2026-10-08 |
| [FIX-01M4E5D447SRCNGAQY4PP006HC](./FIX-01M4E5D447SRCNGAQY4PP006HC.md) | Hors ligne : trois messages et trois gestes pour la même information (C30) | 2026-10-08 |
| [FIX-01M4E5D4JY66T0ETEMRY1DZQZK](./FIX-01M4E5D4JY66T0ETEMRY1DZQZK.md) | Page jamais chargée hors ligne : erreur, deuxième « Réessayer » et « Vu il y a… » sans rien vu (C46) | 2026-10-08 |
| [FIX-01M4E5D51P81MQQYR9PBB3TF22](./FIX-01M4E5D51P81MQQYR9PBB3TF22.md) | Accès révoqué : « Connecte-toi à forge » alors qu'on ne peut pas (C47) | 2026-10-08 |
| [FIX-01M4E5D5Z5MBSD639B5NPX81GC](./FIX-01M4E5D5Z5MBSD639B5NPX81GC.md) | Étape « Empreinte » : quel serveur, où relire l'empreinte, que fait « Refuser » (C3) | 2026-10-08 |
| [FIX-01M4E5D6DYHHX9CBBVDYAFEWMZ](./FIX-01M4E5D6DYHHX9CBBVDYAFEWMZ.md) | Bouton grisé : la raison seulement au survol (C42) | 2026-10-08 |
| [FIX-01M4E48N732694TTFSRQF1C4KG](./FIX-01M4E48N732694TTFSRQF1C4KG.md) | Mise à jour de l'agent : le bouton, l'étape et la mention se répètent (C43) | 2026-10-08 |
| [FIX-01M4E48MY42041VB11FJFSG0XW](./FIX-01M4E48MY42041VB11FJFSG0XW.md) | Port faux : « L'emplacement saisi n'est pas valide » (C4) | 2026-10-08 |
| [FIX-01M4DPR4FFVCMZN0KPEA4JVCC8](./FIX-01M4DPR4FFVCMZN0KPEA4JVCC8.md) | Nombres à l'anglaise et unités brutes (C12) | 2026-10-08 |
| [FIX-01M4DPR45X80GAYPS8MJ4RPZ8E](./FIX-01M4DPR45X80GAYPS8MJ4RPZ8E.md) | Barre des serveurs : outils sans nom, couleurs seulement sur le serveur ouvert, suppression sans nom (C49, C52) | 2026-10-08 |
| [FIX-01M4DPR3YFC0AQC1D3D20KSY04](./FIX-01M4DPR3YFC0AQC1D3D20KSY04.md) | Barres de défilement blanches, une par zone (C9) | 2026-10-08 |
| [FIX-01M4DNJ3P75DN25714EBCH2EY2](./FIX-01M4DNJ3P75DN25714EBCH2EY2.md) | Le rôle en cours de changement ne se distinguait pas des autres cellules (C22) | 2026-10-08 |
| [FIX-01M4DNJ3W899Y50NPSRQ8W8ZVN](./FIX-01M4DNJ3W899Y50NPSRQ8W8ZVN.md) | L'en-tête « Sessions ouvertes » était en chasse fixe, les autres non (C26) | 2026-10-08 |
| [FIX-01M4DNJ42ETVYR5Y01YNVPW715](./FIX-01M4DNJ42ETVYR5Y01YNVPW715.md) | Dans le journal, la recherche ne filtrait qu'avec Entrée ou le bouton éloigné (C28) | 2026-10-08 |
| [FIX-01M4DNJ97KCR694M9JT5B7R67B](./FIX-01M4DNJ97KCR694M9JT5B7R67B.md) | Journal : textes bruts, raison en minuscules, lignes vides, horodatage ISO (C32) | 2026-10-08 |
| [FIX-01M4DJZBAQBTJ55NAA62BNQASH](./FIX-01M4DJZBAQBTJ55NAA62BNQASH.md) | Le menu disait « Sécurité », la page « Sécurité et mode attaque » (C39) | 2026-10-08 |
| [FIX-01M4DNFDC9KF9FXYJ0H2TC2JKX](./FIX-01M4DNFDC9KF9FXYJ0H2TC2JKX.md) | L'effacement en attente se lisait sous une autre session ou chez un non-administrateur | 2026-10-08 |
| [FIX-01M4DJZB43SA08NE46DGEZK213](./FIX-01M4DJZB43SA08NE46DGEZK213.md) | « Plus d'infos » ne donnait aucune information sur l'alerte (C36) | 2026-10-08 |
| [FIX-01M4DJZAXMGAXW5PPQBMQVC96Y](./FIX-01M4DJZAXMGAXW5PPQBMQVC96Y.md) | La marque de sécurité recouvrait les initiales du serveur (C38) | 2026-10-08 |
| [FIX-01M4DJZAPV5ECT10JJ5JWNM8A6](./FIX-01M4DJZAPV5ECT10JJ5JWNM8A6.md) | Les bandeaux de sécurité étaient plus larges que la page, actions à l'autre bout (C35) | 2026-10-08 |
| [FIX-01M4DJZAFYE77R5MKKA2NV6CE3](./FIX-01M4DJZAFYE77R5MKKA2NV6CE3.md) | Le mode attaque éteint parlait au présent et en jargon (C34) | 2026-10-08 |
| [FIX-01M4DKQWTWVNDXFPWJ5SE88SN7](./FIX-01M4DKQWTWVNDXFPWJ5SE88SN7.md) | Les actions d'un compte passaient sur deux lignes à 1280 et 1366 (C21) | 2026-10-08 |
| [FIX-01M4DKQV9ZT6XQF6P7ER02HFYZ](./FIX-01M4DKQV9ZT6XQF6P7ER02HFYZ.md) | Les boutons d'une fenêtre de dialogue sortaient de l'écran à 1100×680 (C20) | 2026-10-08 |
| [FIX-01M4D4Y22QGBWNKKJTK8K3X12A](./FIX-01M4D4Y22QGBWNKKJTK8K3X12A.md) | Le nom accessible du chargement ne suivait pas le texte affiché (retour de revue HRT-34) | 2026-10-08 |
| [FIX-01M4D4Y0D7YFWCV6R1GEHXZGPB](./FIX-01M4D4Y0D7YFWCV6R1GEHXZGPB.md) | Tableau de bord : jauges perdues dans un grand vide à 1920 et 2560 (retour de revue HRT-34) | 2026-10-08 |
| [FIX-01M4D4H25X7PN4SH2N7DC3REGS](./FIX-01M4D4H25X7PN4SH2N7DC3REGS.md) | Les filtres du journal prenaient le quart de l'écran à 1280 et 1366 (C29) | 2026-10-08 |
| [FIX-01M4D4G1E0DGHFXWVPXVW04DF6](./FIX-01M4D4G1E0DGHFXWVPXVW04DF6.md) | « Mes serveurs » : l'adresse recouverte par les étiquettes, barre de défilement au milieu de l'écran (C51) | 2026-10-08 |
| [FIX-01M4D4FZQ6BWTJJK1MYCCYMFDE](./FIX-01M4D4FZQ6BWTJJK1MYCCYMFDE.md) | Beaucoup de serveurs : le bas de la barre sortait de l'écran (C50) | 2026-10-08 |
| [FIX-01M4D4FY4G650RBNJ3W7NSP947](./FIX-01M4D4FY4G650RBNJ3W7NSP947.md) | Mode attaque + alerte : 170 px de bandeaux empilés sur toutes les pages (C37) | 2026-10-08 |
| [FIX-01M4D4FWJBQ360DF7FB9RWFZCM](./FIX-01M4D4FWJBQ360DF7FB9RWFZCM.md) | « Demander mon mot de passe » : le sélecteur se cassait sur trois lignes (C33) | 2026-10-08 |
| [FIX-01M4D4FSDN82D1PNC7YQEERDNB](./FIX-01M4D4FSDN82D1PNC7YQEERDNB.md) | Page Comptes vide : « Comptes » deux fois et « Ajouter un compte » deux fois (C25) | 2026-10-08 |
| [FIX-01M4D4FQRZ2QTRT57HMM6TAPBE](./FIX-01M4D4FQRZ2QTRT57HMM6TAPBE.md) | Le panneau de session expirée se posait en haut à gauche et repoussait la dernière vue (C45) | 2026-10-08 |
| [FIX-01M4D4FP65X3TZW6JQAZ5YK7EX](./FIX-01M4D4FP65X3TZW6JQAZ5YK7EX.md) | Les écrans vides étaient plaqués à gauche, titre cassé, deux tiers de l'écran vides (C27) | 2026-10-08 |
| [FIX-01M4D0RJJNZK7NGDMSFBE18Q29](./FIX-01M4D0RJJNZK7NGDMSFBE18Q29.md) | Deux champs « Mot de passe » identiques à la création d'un compte | 2026-10-08 |
| [FIX-01M4D0RJB17YE0F26QFXGJ2WEV](./FIX-01M4D0RJB17YE0F26QFXGJ2WEV.md) | Un mot de passe de confirmation faux effaçait la saisie du nouveau compte | 2026-10-08 |
| [FIX-01M4D0RJ5EMX3TG1TJB0EJ5EYP](./FIX-01M4D0RJ5EMX3TG1TJB0EJ5EYP.md) | Assistant d'ajout : « Suivant » grisé sans raison et curseur nulle part | 2026-10-08 |
| [FIX-01M4D0RHZE7JFMV700JKA3DM1R](./FIX-01M4D0RHZE7JFMV700JKA3DM1R.md) | Les fenêtres d'actes d'administration s'ouvraient avec le curseur dans la confirmation | 2026-10-08 |
| [FIX-01M4D1K9GV5X6MJTHDB8RYPMS6](./FIX-01M4D1K9GV5X6MJTHDB8RYPMS6.md) | Un chargement des mesures qui ne finit pas se lisait comme une panne (C5) | 2026-10-08 |
| [FIX-01M4D1K8675QMBAFTM76XF4XJC](./FIX-01M4D1K8675QMBAFTM76XF4XJC.md) | La carte Carte graphique absente restait sans explication (C15) | 2026-10-08 |
| [FIX-01M4D1K6V5DS7DQVR6ZV7A3HGY](./FIX-01M4D1K6V5DS7DQVR6ZV7A3HGY.md) | Le titre « Durée de fonctionnement » se cassait en deux lignes et le point de montage des disques était presque invisible (C14) | 2026-10-08 |
| [FIX-01M4D1K5GKA4XPJH8391E1HWKZ](./FIX-01M4D1K5GKA4XPJH8391E1HWKZ.md) | Le tableau de bord ne tenait pas à 1280 px : courbes écrasées, cartes de hauteurs inégales, trous (C6, C7) | 2026-10-08 |
| [FIX-01M4CY8BVM3MV9769QWNDNW7VT](./FIX-01M4CY8BVM3MV9769QWNDNW7VT.md) | Un pic d'une seconde disparaissait de l'historique d'une heure en vieillissant (moyenne par pas de 10 s) | 2026-10-08 |
| [FIX-01M4D6KNXEFG8DH4K9JXBJ98MA](./FIX-01M4D6KNXEFG8DH4K9JXBJ98MA.md) | Un `.erasing` orphelin n'était fini qu'à la mise à jour suivante | 2026-10-08 |
| [FIX-01M4D6KNQRS959ZRJ38TJBQE6N](./FIX-01M4D6KNQRS959ZRJ38TJBQE6N.md) | L'effacement différé des anciennes empreintes n'était visible qu'au journal du service | 2026-10-08 |
| [FIX-01M4D6KNHSZ39EY28XEFAV722X](./FIX-01M4D6KNHSZ39EY28XEFAV722X.md) | L'effacement de reprise des anciennes empreintes était retenté à chaque ouverture de la base | 2026-10-08 |
| [FIX-01M4D6KNC8G4T68J25J7DXMMNM](./FIX-01M4D6KNC8G4T68J25J7DXMMNM.md) | Le client retombait en silence sur les moyennes quand les maxima manquaient | 2026-10-08 |
| [FIX-01M4D6KN655K009FC3JR9H70VN](./FIX-01M4D6KN655K009FC3JR9H70VN.md) | La température tracée sur l'heure restait une moyenne | 2026-10-08 |
| [FIX-01M4CJEQS88NZWCQ129XDP91MT](./FIX-01M4CJEQS88NZWCQ129XDP91MT.md) | Une coupure rétablie entre 1,5 s et 3 s faisait clignoter « Reconnexion en cours » | 2026-10-08 |
| [FIX-01M4CRD60RKGZC2HTT52GK6P2T](./FIX-01M4CRD60RKGZC2HTT52GK6P2T.md) | La série du processeur de la coquille et le repli des dates suivaient l'horloge murale du poste | 2026-10-08 |
| [FIX-01M4CRD4NX3A34B7Z31A7RWE1B](./FIX-01M4CRD4NX3A34B7Z31A7RWE1B.md) | Les mesures de l'agent étaient arrondies à la décimale alors que l'affichage tronque (99,96 devenait 100 %) | 2026-10-08 |
| [FIX-01M4CRD381DKREY9RARJ81E6WH](./FIX-01M4CRD381DKREY9RARJ81E6WH.md) | Une vue sans échantillon (ou plus ancienne) remplaçait l'identité de la machine reçue à la connexion | 2026-10-08 |
| [FIX-01M4CRD1VMNQP4W619XVF1AWRE](./FIX-01M4CRD1VMNQP4W619XVF1AWRE.md) | Un pic rangé avec d'autres échantillons dans un même pas était tracé à la moyenne du pas (100 % tracé à 55 %) | 2026-10-08 |
| [FIX-01M4CRD0HH1YX2RQHBK72GM0VQ](./FIX-01M4CRD0HH1YX2RQHBK72GM0VQ.md) | Un échantillon réellement perdu était comblé par interpolation sur la courbe, contre la règle « un vrai trou reste un trou » | 2026-10-08 |
| [FIX-01M4C9YKCVPDFSSJ2EYMZSRSPM](./FIX-01M4C9YKCVPDFSSJ2EYMZSRSPM.md) | Le point de contrôle occupé de l'effacement des anciennes empreintes n'était pas lu avant de poser la marque | 2026-10-08 |
| [FIX-01M4C9YK78KHZCEH723M9NE8BQ](./FIX-01M4C9YK78KHZCEH723M9NE8BQ.md) | La copie de la base de la mise à jour gardait les anciennes empreintes jusqu'à sa suppression simple | 2026-10-08 |
| [FIX-01M4BZN31A8Z8WN0WKNTCRTFFN](./FIX-01M4BZN31A8Z8WN0WKNTCRTFFN.md) | L'empreinte des requêtes suivies était un SHA-256 sans clé du corps, devinable hors ligne depuis la base (mots de passe) | 2026-10-07 |
| [FIX-01M4BK2JXE7C2SZGG7BBXTZ0TZ](./FIX-01M4BK2JXE7C2SZGG7BBXTZ0TZ.md) | Le poste d'une session était réécrit par toute preuve de session valide sous la clé d'un autre poste du compte | 2026-10-07 |
| [FIX-01M4B118DAFBQZYQX1E5ERY8CA](./FIX-01M4B118DAFBQZYQX1E5ERY8CA.md) | La valeur de démarrage de Windows était écrite sans guillemets autour du chemin (profil avec espace) | 2026-10-07 |
| [FIX-01M47PCYX3BY3YV84R9WW3KAQ3](./FIX-01M47PCYX3BY3YV84R9WW3KAQ3.md) | L'avis de fin de session, arrivé avant la réponse, transformait le résultat d'une action en « résultat inconnu » | 2026-10-06 |
| [FIX-01M47H8VFFS2TNYJ3YNSDZTTKG](./FIX-01M47H8VFFS2TNYJ3YNSDZTTKG.md) | Le carnet gardait l'identifiant tel que saisi à la connexion, pas celui de l'agent | 2026-10-06 |
| [FIX-01M45V0PZB5TRE3A7KQHAHJNXD](./FIX-01M45V0PZB5TRE3A7KQHAHJNXD.md) | Une erreur de lecture de la base était comptée comme « 0 administrateur » | 2026-10-05 |
| [FIX-01M460G91CSXXRV2Q34FXQBYMT](./FIX-01M460G91CSXXRV2Q34FXQBYMT.md) | install.sh suivait une redirection de HTTPS vers HTTP avec wget | 2026-10-05 |
| [FIX-01M460G9AE6AFPSTRF4ZD872QB](./FIX-01M460G9AE6AFPSTRF4ZD872QB.md) | Le retour en arrière d'une installation ne remettait pas l'activation au démarrage, et une erreur de lecture valait « pas activé » | 2026-10-05 |
| [FIX-01M460G9KDDNZAJE3NTJSP49T4](./FIX-01M460G9KDDNZAJE3NTJSP49T4.md) | L'unité systemd retirait CAP_MKNOD : nvidia-smi ne voyait pas la carte d'un serveur sans écran | 2026-10-05 |
| [FIX-01M460G9WVEJW4GPTAZ6MVVC0V](./FIX-01M460G9WVEJW4GPTAZ6MVVC0V.md) | La désinstallation avec purge laissait des temporaires d'écriture (clé privée comprise) et le dossier de données | 2026-10-05 |
| [FIX-01M460GA87EM6ZF9M5R9CWVXW3](./FIX-01M460GA87EM6ZF9M5R9CWVXW3.md) | Un fichier à la place du dossier de données était dit « ouvert à d'autres utilisateurs » | 2026-10-05 |
| [FIX-01M46G7Y2DW32G1D190GE4MA7A](./FIX-01M46G7Y2DW32G1D190GE4MA7A.md) | La troncature des adresses MAC pouvait écarter la carte physique | 2026-10-05 |
| [FIX-01M46G7Z0ZP43T53M2F5KG4VKS](./FIX-01M46G7Z0ZP43T53M2F5KG4VKS.md) | La fin de `logout` effaçait le jeton d'une reconnexion intervenue entre-temps | 2026-10-05 |
| [FIX-01M46G800Z47XQ8R64G2MDC4NP](./FIX-01M46G800Z47XQ8R64G2MDC4NP.md) | « Se souvenir » restait coché sans mot de passe au coffre après une application tuée pendant l'ajout | 2026-10-05 |
| [FIX-01M46N01GMK08NXQHCZ28A2KQ1](./FIX-01M46N01GMK08NXQHCZ28A2KQ1.md) | Une désinstallation lancée juste après une installation pouvait être refusée à tort (verrou relâché trop tard) | 2026-10-05 |
| [FIX-01M47XJXQ0GHV77FN4J6R1NXPZ](./FIX-01M47XJXQ0GHV77FN4J6R1NXPZ.md) | Le filtre d'adresses du téléchargement se contournait avec HTTPS_PROXY | 2026-10-06 |
| [FIX-01M47PHYR8MD87HAXY9PARQXAN](./FIX-01M47PHYR8MD87HAXY9PARQXAN.md) | La patience de la surveillance concluait « échec du superviseur » même quand l'échange des binaires avait eu lieu | 2026-10-06 |
| [FIX-01M47N6Z485TWN2H770KQ5H80R](./FIX-01M47N6Z485TWN2H770KQ5H80R.md) | Une version posée à la main par-dessus une mise à jour laissée en cours était défaite au démarrage (ancien binaire et copie périmée de la base remis) | 2026-10-06 |

À créer lors de tout bugfix conformément à `skills/bugfix/SKILL.md` et `docs/code-rules.md`.

Format fiche :

```markdown
---
id: FIX-{ULID}
titre: Titre court symptôme
date_découverte: 2026-10-04
date_correction: 2026-10-05
---

# FIX-{ULID} — {Titre}

## Symptôme
Comportement incorrect observé.

## Cause root
Explication technique, référence code.

## Impacté
- Versions < X
- Scénario : …

## Workaround
Si aucune correction immédiate.

## Correction
Code changé, commit hash, stratégie (patch, refactoring).

## Références
- Ticket : …
- ADR / BR : …
```

Pointeur code : `// FIX:{ULID}` au-dessus de la ligne corrigée.
