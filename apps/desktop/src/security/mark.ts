import type { SecurityState } from "@/link";
import { alertVisible } from "@/link";

/**
 * Ce que montre la marque d'un serveur dans la barre et la liste (conception design, écran B) : le mode
 * attaque d'abord, puis « suspendu », puis l'alerte. Une seule marque à la fois ; `null` : rien à dire.
 */
export type SecurityMark = "attack" | "suspended" | "alert";

export function markOf(state: SecurityState | null | undefined): SecurityMark | null {
  if (!state) return null;
  if (state.attackMode.state === "active") return "attack";
  if (state.attackMode.state === "suspended") return "suspended";
  return alertVisible(state.alert) ? "alert" : null;
}

/**
 * Minutes qu'il reste avant la reprise d'un mode attaque suspendu, comptées depuis le moment où l'état
 * a été lu (`at`, ms) : l'agent donne des secondes, l'écran dit des minutes (arrondies au-dessus, à la
 * minute, sans zone live). `0` : moins d'une minute. `null` : pas suspendu.
 */
export function resumeMinutes(
  state: SecurityState | null | undefined,
  at: number | null,
  now: number,
): number | null {
  if (state?.attackMode.state !== "suspended" || state.attackMode.resumesInS === null) {
    return null;
  }
  const elapsed = at === null ? 0 : Math.max(0, (now - at) / 1000);
  const remaining = state.attackMode.resumesInS - elapsed;
  return remaining < 60 ? 0 : Math.ceil(remaining / 60);
}
