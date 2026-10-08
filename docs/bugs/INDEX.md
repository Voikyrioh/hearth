# Bugs — Hearth

Fiches d'anomalies découvertes et corrigées : marquage `// FIX:{ULID}` dans le code, fiche explicative `FIX-{ULID}.md` documentant symptôme, cause root, workaround, correction.

| Fiche | Titre | Date |
|---|---|---|
| [FIX-01M4D0RJJNZK7NGDMSFBE18Q29](./FIX-01M4D0RJJNZK7NGDMSFBE18Q29.md) | Deux champs « Mot de passe » identiques à la création d'un compte | 2026-10-08 |
| [FIX-01M4D0RJB17YE0F26QFXGJ2WEV](./FIX-01M4D0RJB17YE0F26QFXGJ2WEV.md) | Un mot de passe de confirmation faux effaçait la saisie du nouveau compte | 2026-10-08 |
| [FIX-01M4D0RJ5EMX3TG1TJB0EJ5EYP](./FIX-01M4D0RJ5EMX3TG1TJB0EJ5EYP.md) | Assistant d'ajout : « Suivant » grisé sans raison et curseur nulle part | 2026-10-08 |
| [FIX-01M4D0RHZE7JFMV700JKA3DM1R](./FIX-01M4D0RHZE7JFMV700JKA3DM1R.md) | Les fenêtres d'actes d'administration s'ouvraient avec le curseur dans la confirmation | 2026-10-08 |
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
