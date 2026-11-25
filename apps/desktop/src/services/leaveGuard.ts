import type { RouteLocationNormalized } from "vue-router";

/** Ce que la garde de sortie de la fiche d'un service juge (HRT-261). */
export type LeaveContext = {
  /** Un lot de mods est en préparation, non appliqué. */
  dirty: boolean;
  /** Une confirmation de sortie est déjà ouverte. */
  confirming: boolean;
  /** Le serveur de cette fiche n'existe plus (retiré de la liste) : il n'y a plus de lot à garder. */
  serverGone: boolean;
};

type Place = Pick<RouteLocationNormalized, "name" | "query"> & {
  redirectedFrom?: unknown;
};

/**
 * Faut-il retenir cette navigation pour demander confirmation ? Non quand rien n'est en préparation, quand une
 * confirmation est déjà ouverte, quand on reste sur le même service, quand la navigation est une REDIRECTION (la
 * garde de l'application, pas un geste de l'admin) ou quand le serveur n'existe plus (le lot n'a plus de sens, la
 * garde ne doit jamais coincer la sortie).
 */
export function holdsNavigation(context: LeaveContext, to: Place, from: Place): boolean {
  if (!context.dirty || context.confirming || context.serverGone) return false;
  if (to.redirectedFrom !== undefined) return false;
  const sameService = to.name === from.name && to.query.service === from.query.service;
  return !sameService;
}
