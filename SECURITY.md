# Politique de sécurité

Hearth pilote un serveur : une faille ici peut donner la main sur une machine. Si tu en trouves une, merci de la signaler en privé plutôt que dans une issue publique.

*English summary: please report vulnerabilities privately through GitHub (Security tab, "Report a vulnerability"), not in a public issue. Details below, in French.*

## Signaler une faille

1. Ouvre l'onglet **Security** de ce dépôt, puis **Report a vulnerability**. Le signalement n'est visible que du mainteneur et de toi.
2. Décris ce que tu as trouvé :
   - la version de Hearth (client et agent) et le système du serveur ;
   - ce qu'un attaquant peut faire, et depuis où (même réseau local, autre compte du serveur, serveur de mise à jour, poste Windows) ;
   - les étapes pour le reproduire, ou une preuve de concept ;
   - si tu en as une, une idée de correction.

N'ouvre pas d'issue publique et n'en parle pas publiquement avant qu'un correctif soit disponible.

## Ce à quoi tu peux t'attendre

Hearth est maintenu par une seule personne, sur son temps libre. Il n'y a ni prime ni délai garanti. L'intention :

- un accusé de réception sous une semaine ;
- un premier avis (confirmé, à creuser, hors périmètre) sous deux semaines ;
- un correctif publié dès qu'il est prêt, avec ton nom dans les notes de version si tu le souhaites.

## Versions concernées

Seule la dernière version publiée reçoit des correctifs. Hearth ne propose jamais de revenir à une version plus ancienne : une faille se corrige par une nouvelle version.

Aucune version n'est encore publiée à ce jour ; la première sera la 0.1.0.

## Ce qui compte comme une faille

- Contourner la connexion, les rôles ou la confirmation des actions d'administration de l'agent.
- Faire accepter par le client ou par l'agent une mise à jour qui n'est pas signée par la clé du projet, ou une version plus ancienne que celle installée.
- Faire accepter au client un serveur dont l'empreinte a changé sans que l'utilisateur le voie.
- Lire ou modifier les comptes, le journal ou les secrets de l'agent sans en avoir le droit.
- Obtenir des droits sur le serveur ou sur le poste Windows par l'installation, la mise à jour ou la désinstallation.
- Faire fuiter un mot de passe, une clé ou une adresse dans un journal, un fichier ou une requête.

## Ce qui n'en est pas une

- Ce que peut faire quelqu'un qui est déjà administrateur du serveur (`root`) ou du poste Windows.
- Un déni de service depuis le réseau local contre son propre serveur.
- L'avertissement Windows à l'installation : l'installateur n'est pas signé par un certificat Windows. Les mises à jour, elles, sont signées et vérifiées.
- Un problème dans une dépendance sans effet démontré sur Hearth : signale-le de préférence au projet concerné.

## Comment Hearth se protège

Pour savoir où chercher, les décisions de sécurité sont écrites dans `docs/adr/` et les règles dans `docs/business-rules/` (connexion, postes de confiance, mode attaque, mises à jour signées, installation). `ARCHITECTURE.md` donne la carte du code.

La confiance repose sur trois choses, aucune n'est dans ce dépôt : la clé de signature des mises à jour, l'empreinte de chaque serveur, et les mots de passe. Le serveur qui distribue les versions n'est pas de confiance : c'est la signature qui fait foi.
