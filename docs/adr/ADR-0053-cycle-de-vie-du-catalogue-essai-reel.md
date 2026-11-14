---
id: ADR-0053
titre: Cycle de vie du catalogue : une fiche est essayée sur sa vraie image avec un vrai moteur avant d'atteindre un utilisateur ; ce qui n'a pas été vu reste `untested` ou `preview`, et le résultat de l'essai est consigné, jamais supposé
type: architecture
statut: proposée (première moitié posée par HRT-114 : le banc d'essai de la fiche Terraria ; relevé `golden`, veille nocturne et registre sont à HRT-115)
date: 2026-11-14
portee: repo
remplace: le brouillon de l'ADR-0053 du hub (2026-10-13), dont ce texte reprend la première moitié
liens: [ADR-0036, ADR-0050, ADR-0051, ADR-0052, conception technique 2026-10-13 (fiches de jeu) sections 12.2 à 12.5, BR-FICHE-006, BR-FICHE-008, BR-FICHE-028, BR-FICHE-037, BR-JEU-038, BR-RES-038]
---

# ADR-0053 : Cycle de vie du catalogue, l'essai réel d'abord

## Contexte
Une fiche (ADR-0050) est écrite d'après un inventaire de documentation et d'une installation de référence : ses ordres d'arrêt, son signe « prêt », ses chiffres de ressources ne sont pas vus par Hearth. Le format sait le dire (`status = "untested"`, `evidence = "unmeasured"`), et l'agent n'exécute ni ne croit jamais un élément `untested` (BR-JEU-038). Reste à lever ces statuts avec une preuve, sans machine de Voiky ni serveur de jeu à la maison.

## Décision
1. **Le banc d'essai réel est un test d'intégration ignoré par défaut** (`crates/hearth-agent/tests/fiche_live.rs`, `HEARTH_TEST_FICHE_LIVE=1`) qui tourne en CI sur le moteur du runner Linux (`.github/workflows/fiche-live.yml`, job `fiche-live-terraria`). Il n'existe aucun essai d'image sur un poste de développement.
2. **L'essai prend le chemin de l'agent**, pas un raccourci : la fiche embarquée, copiée avec ses éléments `untested` passés à `tested` EN MÉMOIRE (« l'essai sur une copie ne change qu'un mot »), est traduite (`translate`), jugée par le validateur du déploiement ordinaire (`body_and_secrets_from`), transformée en plan fermé (`plan_from_spec`), créée et pilotée par l'adaptateur réel (`BollardEngine`), avec les ordres littéraux de la fiche (`send_stdin_line`) et l'arrêt du moteur aussitôt après l'ordre de sortie. Les dossiers gérés et le fichier de réglages sont posés comme l'installation le fait (`SafeFs` et le journal des opérations ne sont pas dans la boucle : limite assumée, couverts par leurs propres tests).
3. **Contrôles, liste fermée** : le signe « prêt » dans les lignes écrites après le lancement ; le port publié accepte une connexion TCP ; un réglage est pris en compte (le monde porte le nom posé) et le jeu n'a pas réécrit le fichier de Hearth ; les ordres de la suite arrivent par l'entrée du conteneur SANS terminal (le jeu sort de lui-même, code 0, avant la coupure forcée) ; l'ordre `save` réécrit le fichier du monde ; le service arrêté reste arrêté (`unless-stopped` + arrêt demandé au moteur) ; le monde existe après l'arrêt ; un redémarrage atteint de nouveau le signe sans créer un autre monde ; mesures au repos : mémoire 60 s après le signe, espace de l'image plus des dossiers gérés.
4. **Le résultat est consigné, pas jugé, pour ce qui décide d'un statut.** L'essai écrit `evidence.toml` (artefact du job et résumé de l'exécution : empreinte de l'image relevée, chaque constat, mesures). Un jeu qui ne lit pas son entrée sans terminal est un RÉSULTAT (la séquence reste `untested`), pas un échec du job ; ce qui fait échouer le job est ce que toute fiche doit tenir : démarrer, répondre, prendre un réglage, rester arrêtée, retrouver son monde.
5. **Un statut ne change que par un relevé.** Le passage de `untested` à `tested` dans `fiche.toml` est un commit qui cite le relevé ; `fiche.toml` pose aussi l'empreinte de l'image relevée (`image.digest`), et la version de la fiche augmente (`catalog-check` l'exige quand le contenu change).
6. **Règle de publication inchangée** (conception 12.5, BR-RES-038) : une fiche reste `preview` tant qu'il manque le relevé de référence des réglages (`golden`) ou une mémoire recommandée mesurée sous charge ; `measured_idle` (le plancher au repos) alimente `min_bytes` et `install_bytes`, jamais une recommandation. `catalog-check --release` refuse toujours une fiche `preview`.
7. **Déclencheurs ciblés** : le job ne tourne que sur les chemins qui peuvent changer son résultat (fiches, banc, chargeur et traduction, arrêt d'une fiche, adaptateur du moteur, le fichier du flux) et à la main ; jamais à chaque push.

## Résultat du premier essai (14/11/2026, `ryshe/terraria:vanilla-1.4.5.8`)
Point B6 levé : Terraria lit son entrée sans terminal. `save` réécrit le monde ; `exit` fait sortir le jeu de lui-même (code 0, moins d'une seconde) ; le service reste arrêté ; le monde survit au redémarrage. Signe « Server started » vu 21 s après le lancement (petit monde). Mémoire au repos 564 Mio, espace installé 53 014 364 octets (image 50 068 911 + dossiers gérés après un premier démarrage) ; empreinte `sha256:3ece8ceb88c11e47bc490d4e17849e89f5343722142fca506fbde9a7ab04a677`.

## Limites
- Un petit monde et aucun joueur : ni charge, ni grand monde, ni reprise d'un monde modifié par un joueur (cela reste à la forge).
- Le fichier du monde est relu par sa taille et sa date, pas par un client de jeu. L'effet de `save` est jugé sur un relevé pris AVANT `exit` (qui enregistre aussi).
- Une fois un statut promu dans la fiche livrée, l'essai l'EXIGE : si l'image ne lit plus son entrée, le job échoue.
- Pas de registre d'empreintes ni de veille nocturne contre l'étiquette la plus récente (HRT-115), pas de relevé `golden` ni de `probe.toml` déclaratif : l'essai est écrit en Rust pour la seule fiche Terraria ; il se généralisera avec la deuxième fiche.
