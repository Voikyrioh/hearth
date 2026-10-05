import { type Directive, type EffectScope, effectScope, shallowRef, watchEffect } from "vue";
import { type MessageKey, t } from "@/i18n";
import type { LinkState } from "@/link";
import { useLinkStore } from "@/stores/link";
import { useServersStore } from "@/stores/servers";

/** Option de la directive : `v-needs-link="{ role: 'admin' }"` exige en plus le rôle administrateur. */
export interface NeedsLinkOptions {
  role?: "admin";
}

const LINK_REASONS: Record<Exclude<LinkState, "connected">, MessageKey> = {
  reconnecting: "needs.reconnecting",
  offline: "needs.offline",
  session_expired: "needs.sessionExpired",
  access_revoked: "needs.accessRevoked",
};

interface Binding {
  scope: EffectScope;
  options: { value: NeedsLinkOptions | undefined };
  onClick: (event: Event) => void;
  saved: { ariaDisabled: string | null; title: string | null };
}

const bindings = new WeakMap<HTMLElement, Binding>();

/**
 * `v-needs-link` : l'unique façon de désactiver une action qui exige le serveur
 * (BR-RESIL-008). Tant que le lien du serveur courant n'est pas « Connecté », ou que le
 * rôle ne suffit pas (`{ role: 'admin' }`), l'élément est inerte : `aria-disabled` (il
 * garde le focus clavier, convention de HButton), clic bloqué avant tout autre
 * gestionnaire, et infobulle qui explique pourquoi. Le retour du lien le réactive seul.
 */
export const vNeedsLink: Directive<HTMLElement, NeedsLinkOptions | undefined> = {
  mounted(el, binding) {
    const servers = useServersStore();
    const link = useLinkStore();
    const options = shallowRef(binding.value);
    const saved = {
      ariaDisabled: el.getAttribute("aria-disabled"),
      title: el.getAttribute("title"),
    };
    const reason = (): MessageKey | null => {
      const server = servers.current;
      if (!server) return "needs.noServer";
      if (options.value?.role === "admin" && server.role !== "admin") return "needs.role";
      const state = link.stateOf(server.id);
      return state === "connected" ? null : LINK_REASONS[state];
    };
    // Capture sur l'élément lui-même : passe avant le `@click` du composant.
    const onClick = (event: Event) => {
      if (reason() === null) return;
      event.preventDefault();
      event.stopImmediatePropagation();
    };
    const scope = effectScope();
    scope.run(() => {
      watchEffect(() => {
        const blocked = reason();
        if (blocked) {
          el.setAttribute("aria-disabled", "true");
          el.setAttribute("title", t(blocked));
          el.dataset.needsLink = blocked;
        } else {
          restore(el, saved);
        }
      });
    });
    el.addEventListener("click", onClick, { capture: true });
    bindings.set(el, { scope, options: options as Binding["options"], onClick, saved });
  },

  updated(el, binding) {
    const entry = bindings.get(el);
    if (entry) entry.options.value = binding.value;
  },

  unmounted(el) {
    const entry = bindings.get(el);
    if (!entry) return;
    entry.scope.stop();
    el.removeEventListener("click", entry.onClick, { capture: true });
    bindings.delete(el);
  },
};

function restore(el: HTMLElement, saved: Binding["saved"]) {
  delete el.dataset.needsLink;
  if (saved.ariaDisabled === null) el.removeAttribute("aria-disabled");
  else el.setAttribute("aria-disabled", saved.ariaDisabled);
  if (saved.title === null) el.removeAttribute("title");
  else el.setAttribute("title", saved.title);
}
