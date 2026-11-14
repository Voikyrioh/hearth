import { onBeforeUnmount, ref } from "vue";
import { useToastsStore } from "@/stores/toasts";

const COPIED_MS = 2000;

/**
 * Copier un texte dans le presse-papiers (HRT-104 : l'adresse à donner aux joueurs, la liste des ports à ouvrir). Un
 * texte copié n'est jamais un secret ici. Le succès dure 2 secondes sur le bouton et se dit aussi par une notification
 * annoncée ; l'échec ne bloque rien, l'appelant dit de copier à la main. Le minuteur s'arrête avec le composant.
 */
export function useCopyText(copiedMessage: () => string) {
  const toasts = useToastsStore();
  const state = ref<"idle" | "copied" | "failed">("idle");
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function copy(text: string): Promise<void> {
    clearTimeout(timer);
    try {
      if (!navigator.clipboard) throw new Error("presse-papiers indisponible");
      await navigator.clipboard.writeText(text);
      state.value = "copied";
      toasts.push({ kind: "success", message: copiedMessage() });
      timer = setTimeout(() => {
        state.value = "idle";
      }, COPIED_MS);
    } catch {
      state.value = "failed";
    }
  }

  onBeforeUnmount(() => clearTimeout(timer));
  return { state, copy };
}
