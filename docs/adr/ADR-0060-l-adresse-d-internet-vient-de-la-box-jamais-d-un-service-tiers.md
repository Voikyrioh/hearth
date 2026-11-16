# ADR-0060 : L'adresse d'internet d'un serveur de jeu vient du propriétaire ou de la box, jamais d'un service tiers

Statut : acceptée (2026-11-16, HRT-223)

## Contexte
Le détail d'un serveur de jeu montrait une seule adresse. Les joueurs du réseau maison et ceux d'internet n'utilisent pas la même. Aucun mécanisme n'existait pour connaître l'adresse d'internet : HRT-215 permet seulement de la saisir. Hearth est un produit de maison, public et libre : il ne doit dépendre d'aucun service d'écho d'IP ni révéler l'adresse du foyer à un tiers.

## Décision
- L'agent lit l'adresse du réseau maison sur la machine (interface qu'utiliserait une connexion sortante ; aucun paquet envoyé).
- L'agent demande l'adresse d'internet à la box du foyer par UPnP-IGD (recherche SSDP en multidiffusion locale, description de la box, requête SOAP `GetExternalIPAddress`), avec un module de quelques dizaines de lignes écrit à la main (pas de crate : surface réduite, aucune dépendance de plus). Seule la passerelle par défaut (`/proc/net/route`) est interrogée : réponse et description ne sont crues que si leur hôte est exactement elle (aucune autre machine du réseau ne peut imposer une adresse ni faire viser une autre cible) ; description en `http`, chemin sûr, lectures bornées ; la découverte est unique, en tâche de fond, et son résultat gardé 10 minutes.
- Le client montre les deux adresses ; l'adresse renseignée par le propriétaire (HRT-215) passe devant celle de la box.
- Aucune requête vers un service tiers, sous aucun prétexte. Si la box ne répond pas, le propriétaire saisit un nom de domaine ou une adresse.

## Conséquences
- Ça marche sur la plupart des box domestiques ; pas derrière un double NAT, ni avec UPnP désactivé : la saisie reste le recours.
- L'adresse dite par la box peut changer (adresse dynamique) : la carte conseille un nom de domaine.
- Hearth ne vérifie pas la redirection du port (aucun test depuis l'extérieur). Une lecture des redirections UPnP (`GetSpecificPortMappingEntry`) pourrait dire « port redirigé » plus tard, sans service externe.
- Règle : BR-JEU-043.

## Alternatives écartées
- Un service d'écho d'IP (api.ipify.org et similaires) : fuit l'adresse du foyer, dépendance externe.
- Une crate UPnP (`igd-next`) : plus de code transitif pour trois échanges simples.
