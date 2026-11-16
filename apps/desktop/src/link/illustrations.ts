/**
 * Les adresses des images de jeux (HRT-239, ADR-0061, BR-FICHE-062), des fonctions PURES : aucune E/S, aucun réseau.
 * La page ne charge jamais une image elle-même : elle écrit `<img src="hearth-illustration://localhost/...">`, protocole
 * de la coquille, qui lit son cache ou va chercher l'image (Steam seulement, si le réglage le permet). L'adresse n'est
 * construite que depuis un entier validé (Steam) ou un identifiant de serveur, de fiche et une empreinte (agent) ; une
 * valeur qui n'a pas la forme attendue donne `""` (la page montre alors le visuel neutre, rien ne part).
 */

/** Sous Windows, le protocole d'URI personnalisé de Tauri s'écrit `http://<protocole>.localhost/`. */
export function illustrationBase(windows: boolean): string {
  return windows ? "http://hearth-illustration.localhost" : "hearth-illustration://localhost";
}

const U32_MAX = 4_294_967_295;
const STABLE_ID = /^[a-z0-9_-]{1,64}$/;
const SERVER_ID = /^[A-Za-z0-9_-]{1,64}$/;
const HEX_DIGEST = /^[0-9a-f]{64}$/;

/** L'identifiant Steam d'un jeu : un entier de 1 à 4 294 967 295 (jamais une chaîne, jamais 0). */
export function isSteamAppId(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 1 && value <= U32_MAX;
}

export function steamHeaderUrl(base: string, appId: number): string {
  return isSteamAppId(appId) ? `${base}/steam/${appId}/header` : "";
}

/** `digest` : `sha256:<64 hexadécimaux>` tel que la liste le donne (ou les 64 caractères seuls). */
export function agentIllustrationUrl(
  base: string,
  serverId: string,
  ficheId: string,
  digest: string,
): string {
  const hex = digest.startsWith("sha256:") ? digest.slice("sha256:".length) : digest;
  if (!SERVER_ID.test(serverId) || !STABLE_ID.test(ficheId) || !HEX_DIGEST.test(hex)) return "";
  return `${base}/agent/${serverId}/${ficheId}/${hex}`;
}
