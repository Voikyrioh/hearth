# Bugs — Hearth

Fiches d'anomalies découvertes et corrigées : marquage `// FIX:{ULID}` dans le code, fiche explicative `FIX-{ULID}.md` documentant symptôme, cause root, workaround, correction.

| Fiche | Titre | Date |
|---|---|---|
| [FIX-01M4BK2JXE7C2SZGG7BBXTZ0TZ](./FIX-01M4BK2JXE7C2SZGG7BBXTZ0TZ.md) | Le poste d'une session était réécrit par toute preuve de session valide sous la clé d'un autre poste du compte | 2026-10-07 |
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
