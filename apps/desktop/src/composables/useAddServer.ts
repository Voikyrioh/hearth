import { computed, ref } from "vue";
import { type MessageKey, t } from "@/i18n";
import {
  failureMessage,
  failureOf,
  getLinkBridge,
  type LinkFailure,
  type ProbeResult,
  type ServerColor,
  type ServerInfo,
} from "@/link";
import { useServersStore } from "@/stores/servers";
import { useToastsStore } from "@/stores/toasts";
import { hostError, nameError, parsePort, portError, serverAt } from "@/validation/server";
import { useCountdown } from "./useCountdown";

export type AddStep = "address" | "fingerprint" | "login";

/** Les trois temps, dans l'ordre (index = position dans le fil des étapes). */
export const ADD_STEPS: readonly AddStep[] = ["address", "fingerprint", "login"];

/**
 * L'assistant d'ajout de serveur en 3 temps (BR-CONN-001, 002, 004, 011, 012) :
 * 1. nom, adresse, port, couleur : « Suivant » sonde l'adresse (aucun identifiant n'est envoyé) ;
 * 2. l'empreinte s'affiche : « Confirmer » passe aux identifiants, « Refuser » n'ajoute rien ;
 * 3. identifiant et mot de passe : « Se connecter » contacte le serveur sur l'empreinte confirmée,
 *    ouvre la session et SEULEMENT si elle réussit enregistre le serveur, son empreinte et ses
 *    secrets. Tant que la connexion n'a pas réussi rien n'existe hors de l'écran : « Précédent »,
 *    « Annuler » ou fermer la fenêtre ne laissent rien derrière eux ; une application tuée pendant l'écriture laisse au
 *    pire un serveur sans session (visible, supprimable), jamais un secret orphelin.
 * Toute la logique est ici, le composant `AddServerWizard` ne fait qu'afficher.
 */
export function useAddServer() {
  const servers = useServersStore();
  const toasts = useToastsStore();
  const countdown = useCountdown();

  const step = ref<AddStep>("address");
  const name = ref("");
  const host = ref("");
  const port = ref("");
  const color = ref<ServerColor>(1);
  const touched = ref({ name: false, host: false, port: false });
  const probe = ref<ProbeResult | null>(null);
  const busy = ref<"probe" | "login" | null>(null);
  /** Échec de la sonde ou de l'enregistrement, affiché sous le champ de l'adresse. */
  const hostFailure = ref<string | null>(null);
  /** Échec du nom (déjà pris, vu par la liaison), affiché sous le champ du nom. */
  const nameFailure = ref<string | null>(null);
  /** Message de carte (versions incompatibles, erreur générale). */
  const cardMessage = ref<string | null>(null);
  /** Serveur déjà enregistré à cette adresse : on propose d'ouvrir l'entrée existante. */
  const existing = ref<ServerInfo | null>(null);
  const loginError = ref<string | null>(null);

  const othersNames = computed(() => servers.servers);
  const errors = computed(() => ({
    name:
      nameFailure.value ??
      (touched.value.name ? key(nameError(name.value, othersNames.value)) : null),
    host: hostFailure.value ?? (touched.value.host ? key(hostError(host.value)) : null),
    port: touched.value.port ? key(portError(port.value)) : null,
  }));
  /** « Suivant » n'est actif que si le nom, l'adresse et le port sont remplis et valides. */
  const canNext = computed(
    () =>
      nameError(name.value, othersNames.value) === null &&
      hostError(host.value) === null &&
      portError(port.value) === null,
  );

  function key(message: MessageKey | null): string | null {
    return message === null ? null : t(message);
  }

  function clearMessages() {
    hostFailure.value = null;
    nameFailure.value = null;
    cardMessage.value = null;
    existing.value = null;
    loginError.value = null;
  }

  /** Une saisie change : les échecs de la tentative précédente ne valent plus. */
  function edited() {
    hostFailure.value = null;
    nameFailure.value = null;
    cardMessage.value = null;
    existing.value = null;
  }

  function failureText(failure: LinkFailure): string {
    return failureMessage(failure, countdown.remaining.value || undefined);
  }

  /** Temps 1 vers 2 : sonde l'adresse. */
  async function next(): Promise<void> {
    touched.value = { name: true, host: true, port: true };
    clearMessages();
    if (!canNext.value || busy.value) return;
    const portNumber = parsePort(port.value) ?? null;
    const known = serverAt(servers.servers, host.value, portNumber);
    if (known) {
      existing.value = known;
      return;
    }
    busy.value = "probe";
    try {
      probe.value = await getLinkBridge().probeServer(host.value.trim(), portNumber);
      step.value = "fingerprint";
    } catch (error) {
      const failure = failureOf(error);
      if (failure?.kind === "incompatible_agent" || failure?.kind === "incompatible_client") {
        cardMessage.value = failureText(failure);
      } else {
        hostFailure.value = failure ? failureText(failure) : t("failure.generic");
      }
    } finally {
      busy.value = null;
    }
  }

  /** Temps 2 : l'utilisateur confirme l'empreinte ; elle est enregistrée avec le serveur à la connexion. */
  function confirm(): void {
    if (probe.value && !busy.value) step.value = "login";
  }

  /** Temps 2 : « Refuser » : rien n'est enregistré, aucun identifiant n'est parti. */
  function refuse(): void {
    probe.value = null;
    step.value = "address";
  }

  /** Temps 3 vers 1 : « Précédent ». Rien n'existe encore : on revient simplement aux saisies. */
  function back(): void {
    if (busy.value) return;
    probe.value = null;
    loginError.value = null;
    step.value = "address";
  }

  /** Temps 3 : connecte et enregistre. Rend l'identifiant du serveur en cas de succès. */
  async function login(entry: {
    username: string;
    password: string;
    remember: boolean;
  }): Promise<string | null> {
    const found = probe.value;
    if (!found || busy.value || countdown.active.value) return null;
    loginError.value = null;
    busy.value = "login";
    try {
      const server = await getLinkBridge().addAndLogin({
        name: name.value.trim(),
        color: color.value,
        host: host.value.trim(),
        port: parsePort(port.value) ?? null,
        fingerprint: found.fingerprint,
        macAddresses: found.macAddresses,
        username: entry.username,
        password: entry.password,
        remember: entry.remember,
      });
      toasts.push({ kind: "success", message: t("connect.connectedTo", { name: server.name }) });
      return server.id;
    } catch (error) {
      const failure = failureOf(error);
      if (failure?.kind === "too_many_attempts") {
        countdown.start(failure.retry_after_s);
      } else if (failure?.kind === "name_taken") {
        // Un autre serveur du même nom est apparu : retour aux saisies, nom signalé.
        probe.value = null;
        step.value = "address";
        nameFailure.value = t("validation.nameTaken");
      } else if (failure?.kind === "already_exists") {
        probe.value = null;
        step.value = "address";
        existing.value =
          serverAt(servers.servers, host.value, parsePort(port.value) ?? null) ?? null;
        cardMessage.value = existing.value ? null : t("connect.existing");
      } else if (
        failure?.kind === "fingerprint_changed" ||
        failure?.kind === "verification_required"
      ) {
        // Le serveur ne présente plus l'empreinte confirmée : on recommence la vérification.
        probe.value = null;
        step.value = "address";
        cardMessage.value = t("failure.generic");
      } else {
        loginError.value = failure ? failureText(failure) : t("failure.generic");
      }
      return null;
    } finally {
      busy.value = null;
    }
  }

  return {
    step,
    name,
    host,
    port,
    color,
    touched,
    probe,
    busy,
    errors,
    canNext,
    cardMessage,
    existing,
    loginError,
    lockedSeconds: countdown.remaining,
    edited,
    next,
    confirm,
    refuse,
    back,
    login,
  };
}
