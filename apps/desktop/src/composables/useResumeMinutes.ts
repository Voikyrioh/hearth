import { computed } from "vue";
import { useNow } from "@/composables/useNow";
import { resumeMinutes } from "@/security/mark";
import type { ServerSecurity } from "@/stores/security";

/** Minutes avant la reprise du mode attaque suspendu (0 : moins d'une minute), recalculées avec l'horloge partagée. */
export function useResumeMinutes(entry: () => ServerSecurity | undefined) {
  const now = useNow();
  return computed(() => resumeMinutes(entry()?.state, entry()?.at ?? null, now.value));
}
