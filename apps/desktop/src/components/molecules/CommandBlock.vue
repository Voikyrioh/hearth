<script setup lang="ts">
import { onBeforeUnmount, ref } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HTag from "@/components/atoms/HTag.vue";
import { t } from "@/i18n";
import Callout from "./Callout.vue";

// Une commande, ou un texte de configuration, À RECOPIER sur le serveur (BR-ENGINE-017) : Hearth ne
// l'exécute jamais. Le bloc défile dans son cadre (jamais la page), reçoit le focus clavier pour défiler et
// reste sélectionnable. « Copier » reste actif même lien coupé (le texte est déjà là) ; « Copié » dure 2
// secondes et est annoncé ; un échec dit de copier à la main, sans bloquer (aucun secret n'est concerné).
// `label`, `copyLabel`, `copiedLabel`, `failedLabel` remplacent les textes du moteur de conteneurs (le droit de
// gérer les services a les siens : « Commande à lancer sur le serveur », « Copiée »).
const props = withDefaults(
  defineProps<{
    text: string;
    config?: boolean;
    label?: string;
    copyLabel?: string;
    copiedLabel?: string;
    failedLabel?: string;
  }>(),
  {
    config: false,
    label: undefined,
    copyLabel: undefined,
    copiedLabel: undefined,
    failedLabel: undefined,
  },
);

const COPIED_MS = 2000;
const state = ref<"idle" | "copied" | "failed">("idle");
let timer: ReturnType<typeof setTimeout> | undefined;

async function copy() {
  clearTimeout(timer);
  try {
    if (!navigator.clipboard) throw new Error("presse-papiers indisponible");
    await navigator.clipboard.writeText(props.text);
    state.value = "copied";
    timer = setTimeout(() => {
      state.value = "idle";
    }, COPIED_MS);
  } catch {
    state.value = "failed";
  }
}

onBeforeUnmount(() => clearTimeout(timer));
</script>

<template>
  <div class="block">
    <div class="block__head">
      <HTag>{{ label ?? (config ? t("engine.addHere") : t("engine.runHere")) }}</HTag>
      <HButton variant="secondary" size="sm" data-copy @click="copy">
        <HIcon v-if="state === 'copied'" name="check" size="sm" />
        <HIcon v-else name="copy" size="sm" />
        {{
          state === "copied"
            ? (copiedLabel ?? t("engine.copied"))
            : (copyLabel ?? (config ? t("engine.copyText") : t("engine.copyCommand")))
        }}
      </HButton>
    </div>
    <pre
      class="block__text"
      tabindex="0"
      :aria-label="config ? t('engine.textLabel') : t('engine.commandLabel')"
    >{{ text }}</pre>
    <span class="block__live" role="status">{{
      state === "copied" ? (copiedLabel ?? t("engine.copied")) : ""
    }}</span>
    <Callout v-if="state === 'failed'" tone="warn" inline>
      {{
        failedLabel ?? (config ? t("engine.copyFailedText") : t("engine.copyFailedCommand"))
      }}
    </Callout>
  </div>
</template>

<style scoped>
.block {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
}

.block__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.block__text {
  max-height: var(--command-max-height);
  margin: 0;
  padding: var(--space-3) var(--space-4);
  overflow: auto;
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--bg);
  color: var(--tx);
  font-family: var(--font-mono);
  font-size: var(--fs-body);
  user-select: text;
  white-space: pre;
}

.block__live {
  position: absolute;
  width: var(--border-width);
  height: var(--border-width);
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
</style>
