<script setup lang="ts">
import { computed, ref, watch } from "vue";
import HSegmented from "@/components/atoms/HSegmented.vue";
import SettingRow from "@/components/molecules/SettingRow.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import type { ActReport } from "@/composables/useReauth";
import { useServerAction } from "@/composables/useServerAction";
import { t } from "@/i18n";
import { getLinkBridge, type ReauthMode, type ReauthState } from "@/link";
import { useToastsStore } from "@/stores/toasts";

// Ligne de réglage « Demander mon mot de passe » de la page Sécurité (HRT-30, D2, Q19) : « Toutes les
// 5 minutes » (défaut) ou « À chaque action ». La valeur est LUE de l'agent (réglage du compte, tenu par
// l'agent) ; la changer est elle-même un acte d'administration, toujours confirmé (mot de passe, jamais
// couvert par le délai, et clé de ce PC) par la fenêtre commune des actes. Absente face à un agent qui n'annonce pas
// la confirmation des actes : il n'y a rien à régler.
const props = defineProps<{ serverId: string }>();

const toasts = useToastsStore();
const action = useServerAction();
const state = ref<ReauthState | null>(null);
const asked = ref<ReauthMode | null>(null);

async function load() {
  try {
    state.value = await getLinkBridge().getReauthState(props.serverId);
  } catch {
    // Illisible (lien coupé) : la ligne garde ce qu'elle sait, la page relit au retour du lien.
  }
}
watch(() => props.serverId, load, { immediate: true });

const OPTIONS = computed(() => [
  { value: "window" as const, label: t("reauth.settingWindow") },
  { value: "each" as const, label: t("reauth.settingEach") },
]);

async function perform(adminPassword: string | null): Promise<ActReport> {
  const mode = asked.value;
  if (!mode) return { kind: "failed" };
  const result = await action.run(
    () => getLinkBridge().setReauthSetting(props.serverId, mode, adminPassword ?? ""),
    { forbiddenMessage: "security.noPermission" },
  );
  if (!result) return { kind: "failed" };
  if (result.kind === "done") {
    toasts.push({
      kind: "success",
      message: t(
        result.mode === "each" ? "reauth.settingChangedEach" : "reauth.settingChangedWindow",
      ),
    });
    await load();
    return { kind: "done" };
  }
  if (result.kind === "refused") return { kind: "refused", refusal: result.refusal };
  return { kind: "unknown" };
}

const refusalText = (refusal: { kind: string }) =>
  refusal.kind === "unsupported" ? t("reauth.settingUnsupported") : t("failure.generic");
</script>

<template>
  <section v-if="state?.supported" class="setting" data-reauth-setting>
    <SettingRow :label="t('reauth.settingLabel')" :help="t('reauth.settingHelp')">
      <HSegmented
        :model-value="state.mode"
        :options="OPTIONS"
        :label="t('reauth.settingLabel')"
        @update:model-value="asked = $event"
      />
    </SettingRow>
    <AdminActDialog
      :open="asked !== null && asked !== state.mode"
      :server-id="serverId"
      kind="reauth_setting"
      :title="t('reauth.settingTitle')"
      :submit-label="t('reauth.settingApply')"
      :perform="perform"
      :refusal-text="refusalText"
      @close="asked = null"
    >
      <p>{{ t(asked === 'each' ? 'reauth.settingMessageEach' : 'reauth.settingMessageWindow') }}</p>
    </AdminActDialog>
  </section>
</template>

<style scoped>
.setting {
  padding: var(--space-3) var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}
</style>
