import { computed, ref } from "vue";
import { t } from "@/i18n";
import {
  DEFAULT_PORT,
  failureMessage,
  failureOf,
  getLinkBridge,
  type ProbeResult,
  type ServerColor,
  type ServerInfo,
} from "@/link";
import { useServersStore } from "@/stores/servers";
import { hostError, nameError, parsePort, portError } from "@/validation/server";

/**
 * Modification d'un serveur enregistré (nom, couleur, adresse). Une autre adresse passe par la
 * sonde puis la confirmation de l'empreinte lue à cette adresse avant d'enregistrer (BR-CONN-009) ;
 * les identifiants mémorisés sont conservés. Toute la logique est ici, `ServerEditForm` affiche.
 */
export function useEditServer(server: () => ServerInfo) {
  const servers = useServersStore();
  const initial = server();
  const name = ref(initial.name);
  const host = ref(initial.host);
  const port = ref(initial.port === DEFAULT_PORT ? "" : String(initial.port));
  const color = ref<ServerColor>(initial.color);
  const step = ref<"form" | "verify">("form");
  const probe = ref<ProbeResult | null>(null);
  const busy = ref(false);
  const failure = ref<string | null>(null);

  const others = computed(() => servers.servers.filter((other) => other.id !== server().id));
  const messages = computed(() => {
    const nameKey = nameError(name.value, others.value);
    const hostKey = hostError(host.value);
    const portKey = portError(port.value);
    return {
      name: nameKey ? t(nameKey) : undefined,
      host: hostKey ? t(hostKey) : undefined,
      port: portKey ? t(portKey) : undefined,
    };
  });
  const valid = computed(
    () => !messages.value.name && !messages.value.host && !messages.value.port,
  );
  const portNumber = computed(() => parsePort(port.value) ?? null);
  /** L'adresse change : l'empreinte doit être relue et confirmée de nouveau. */
  const moved = computed(
    () =>
      host.value.trim().toLowerCase() !== server().host.toLowerCase() ||
      (portNumber.value ?? DEFAULT_PORT) !== server().port,
  );

  async function save(fingerprint: string | null): Promise<ServerInfo> {
    return getLinkBridge().updateServer(server().id, {
      name: name.value.trim(),
      color: color.value,
      host: host.value.trim(),
      port: portNumber.value,
      fingerprint,
    });
  }

  function fail(error: unknown) {
    const reason = failureOf(error);
    failure.value = reason ? failureMessage(reason) : t("failure.generic");
  }

  /** « Enregistrer » ou « Suivant » : rend le serveur modifié si rien d'autre n'est à confirmer. */
  async function submit(): Promise<ServerInfo | null> {
    if (!valid.value || busy.value) return null;
    failure.value = null;
    busy.value = true;
    try {
      if (!moved.value) return await save(null);
      probe.value = await getLinkBridge().probeServer(host.value.trim(), portNumber.value);
      step.value = "verify";
      return null;
    } catch (error) {
      fail(error);
      return null;
    } finally {
      busy.value = false;
    }
  }

  /** « Confirmer » l'empreinte lue à la nouvelle adresse : enregistre. */
  async function confirm(): Promise<ServerInfo | null> {
    if (!probe.value || busy.value) return null;
    failure.value = null;
    busy.value = true;
    try {
      return await save(probe.value.fingerprint);
    } catch (error) {
      fail(error);
      step.value = "form";
      return null;
    } finally {
      busy.value = false;
    }
  }

  /** « Refuser » : retour au formulaire, rien n'est modifié. */
  function refuse() {
    probe.value = null;
    step.value = "form";
  }

  return {
    name,
    host,
    port,
    color,
    step,
    probe,
    busy,
    failure,
    messages,
    valid,
    moved,
    submit,
    confirm,
    refuse,
  };
}
