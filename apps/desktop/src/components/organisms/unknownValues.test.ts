import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import type { Account, AttackMode } from "@/link";
import { modeOn, modeUnknown } from "@/link";
import { attackModeBlock, blockMessageKey } from "@/security/gate";
import { useSecurityStore } from "@/stores/security";
import { useServersStore } from "@/stores/servers";
import { useToastsStore } from "@/stores/toasts";
import { startedApp } from "@/test/app";
import AccountTable from "./AccountTable.vue";
import AttackModeBanner from "./AttackModeBanner.vue";
import AttackModePanel from "./AttackModePanel.vue";
import PasswordDialog from "./PasswordDialog.vue";
import ReauthSettingCard from "./ReauthSettingCard.vue";
import SecurityAlertBanner from "./SecurityAlertBanner.vue";
import SecurityAlertCard from "./SecurityAlertCard.vue";

// BR-COMPAT-001 point 4 : une valeur que ce client ne connaît pas (agent plus récent) n'est JAMAIS prise pour une valeur
// connue, et ne déclenche jamais un geste : un mode attaque inconnu n'est ni « activé » ni « inactif », sans bouton ni
// bandeau qui affirme une attaque ; un rôle inconnu n'est ni lecture seule ni administrateur.

afterEach(() => {
  document.body.innerHTML = "";
});

const UNKNOWN_MODE: AttackMode = { state: "unknown", since: null, resumesInS: null, lastEnd: null };
const NO_READ_TEXT =
  "Ce client ne sait pas lire l'état de protection de ce serveur. Mets Hearth à jour sur ce PC.";

function account(partial: Partial<Account> & { username: string }): Account {
  return {
    id: partial.username.toUpperCase(),
    role: "readonly",
    createdAt: "2026-10-04T10:30:15.250Z",
    lastLoginAt: null,
    sessionsOpen: 0,
    managesServices: false,
    ...partial,
  };
}

describe("un mode attaque que ce client ne sait pas lire", () => {
  it("n'est pas en vigueur, et se reconnaît", () => {
    expect(modeOn(UNKNOWN_MODE)).toBe(false);
    expect(modeUnknown(UNKNOWN_MODE)).toBe(true);
    expect(modeOn({ ...UNKNOWN_MODE, state: "active" })).toBe(true);
    expect(modeOn({ ...UNKNOWN_MODE, state: "suspended" })).toBe(true);
    expect(modeOn({ ...UNKNOWN_MODE, state: "off" })).toBe(false);
    expect(modeUnknown({ ...UNKNOWN_MODE, state: "off" })).toBe(false);
  });

  it("se dit « Inconnu » sans aucun bouton et sans « Actif » ni « Inactif »", async () => {
    const { pinia } = await startedApp();
    const wrapper = mount(AttackModePanel, {
      props: { mode: UNKNOWN_MODE, block: null, minutes: null },
      global: { plugins: [pinia] },
      attachTo: document.body,
    });
    expect(wrapper.get("[data-attack-mode-state]").text()).toBe("Inconnu");
    expect(wrapper.text()).toContain(NO_READ_TEXT);
    expect(wrapper.find("button").exists()).toBe(false);
    expect(wrapper.find("[data-attack-mode-toggle]").exists()).toBe(false);
    expect(wrapper.find("[data-attack-mode-details]").exists()).toBe(false);
    for (const claim of ["Actif", "Inactif", "Suspendu", "Désactiver", "Activer"]) {
      expect(wrapper.text()).not.toContain(claim);
    }
    wrapper.unmount();
  });

  it("garde l'alerte visible mais ne propose pas « Activer » sur un état inconnu", async () => {
    const { pinia } = await startedApp();
    const alert = { own: true, since: null, others: null };
    const banner = mount(SecurityAlertBanner, {
      props: { alert, role: "admin", block: null, modeOn: false, modeUnknown: true },
      global: { plugins: [pinia] },
    });
    expect(banner.find("[data-security-alert]").exists()).toBe(true);
    expect(banner.find("[data-security-activate]").exists()).toBe(false);
    const known = mount(SecurityAlertBanner, {
      props: { alert, role: "admin", block: null, modeOn: false },
      global: { plugins: [pinia] },
    });
    expect(known.find("[data-security-activate]").exists()).toBe(true);
    const card = mount(SecurityAlertCard, {
      props: { alert, role: "admin", serverId: "forge", modeOn: false, modeUnknown: true },
      global: { plugins: [pinia], stubs: { RouterLink: true } },
    });
    const todo = card.get("[data-alert-todo]").text();
    expect(todo).toContain("Ce client ne sait pas lire l'état de protection");
    expect(todo).not.toContain("déjà actif");
    expect(todo).not.toContain("active le mode attaque");
  });

  it("garde un bandeau permanent, neutre, sans bouton d'action sur le mode attaque", async () => {
    const { pinia } = await startedApp();
    const wrapper = mount(AttackModeBanner, {
      props: { suspended: false, minutes: null, unknown: true },
      global: { plugins: [pinia] },
    });
    const banner = wrapper.get("[data-attack-mode-banner]");
    expect(banner.classes()).toContain("banner--neutral");
    expect(banner.text()).toContain("État de protection illisible");
    expect(banner.text()).toContain(NO_READ_TEXT);
    expect(banner.text()).not.toContain("Mode attaque actif");
    expect(banner.text()).not.toContain("Seuls les postes reconnus");
    // Seul geste : ouvrir la page Sécurité, jamais activer ni désactiver.
    expect(wrapper.findAll("button").map((b) => b.text())).toEqual(["Voir la page Sécurité"]);
    wrapper.unmount();
  });

  // FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB : un état inconnu ne fait jamais taire l'avertissement « poste sans clé ».
  describe("l'avertissement « poste sans clé » du changement de mot de passe", () => {
    async function warningFor(mode: "off" | "active" | "unknown", keyAtHand: boolean) {
      const { bridge, pinia } = await startedApp();
      useServersStore().setCurrent("forge");
      bridge.security.setMode("forge", mode);
      const security = useSecurityStore();
      await security.load("forge");
      // La fenêtre se bloque d'elle-même sur un poste sans clé au coffre (autre source) : on pose seulement
      // ici ce que la sécurité dit du poste, pour éprouver l'avertissement.
      const entry = security.of("forge");
      if (entry?.state)
        security.byServer.forge = { ...entry, state: { ...entry.state, keyAtHand } };
      const wrapper = mount(PasswordDialog, {
        props: { open: true, serverId: "forge", username: "marie", own: true },
        global: { plugins: [pinia] },
        attachTo: document.body,
      });
      await flushPromises();
      const warning = document.querySelector("[data-keep-address-warn]")?.textContent?.trim();
      wrapper.unmount();
      return warning;
    }

    it("est montré, au conditionnel, quand le mode attaque est inconnu et que le poste n'a pas de clé", async () => {
      const warning = await warningFor("unknown", false);
      expect(warning).toContain("ne sait pas lire l'état du mode attaque");
      expect(warning).toContain("si le mode attaque est actif");
      expect(warning).not.toContain("Le mode attaque est actif et");
    });

    it("garde sa formulation ferme quand le mode est connu actif, et se tait quand il est inactif ou que le poste a une clé", async () => {
      expect(await warningFor("active", false)).toContain("Le mode attaque est actif et ce poste");
      expect(await warningFor("off", false)).toBeUndefined();
      expect(await warningFor("unknown", true)).toBeUndefined();
    });
  });

  it("n'annonce pas une fin automatique d'un mode qu'on ne savait pas actif", async () => {
    const { bridge } = await startedApp();
    const security = useSecurityStore();
    await security.start();
    await security.load("forge");
    const toasts = useToastsStore();
    bridge.security.setMode("forge", "unknown");
    await flushPromises();
    const before = toasts.items.length;
    bridge.security.setMode("forge", "off", { lastEnd: "auto" });
    await flushPromises();
    expect(toasts.items.length).toBe(before);
    // Contre-épreuve : depuis un mode actif connu, la fin automatique est annoncée.
    bridge.security.setMode("forge", "active");
    await flushPromises();
    bridge.security.setMode("forge", "off", { lastEnd: "auto" });
    await flushPromises();
    expect(toasts.items.length).toBe(before + 1);
  });
});

describe("un rôle que ce client ne connaît pas", () => {
  it("n'est pas dit « lecture seule » : le mode attaque dit pourquoi il n'est pas proposé", () => {
    expect(attackModeBlock("unknown", undefined)).toBe("role_unknown");
    expect(blockMessageKey("role_unknown")).toBe("security.roleUnknown");
    expect(attackModeBlock("readonly", undefined)).toBe("readonly");
  });

  it("n'est pas dit « lecture seule » dans la bannière d'alerte, et ne propose rien", async () => {
    const { pinia } = await startedApp();
    const alert = { own: true, since: null, others: null };
    const banner = mount(SecurityAlertBanner, {
      props: { alert, role: "unknown", block: "role_unknown" },
      global: { plugins: [pinia] },
    });
    const text = banner.text();
    expect(text).toContain("ne reconnaît pas ton rôle");
    expect(text).not.toContain("seul un administrateur");
    const activate = banner.find("[data-security-activate]");
    if (activate.exists()) {
      const element = activate.element as HTMLButtonElement;
      expect(element.disabled || element.getAttribute("aria-disabled") === "true").toBe(true);
    }
  });

  it("ne propose aucun changement de rôle pour un compte au rôle inconnu", async () => {
    const { pinia } = await startedApp();
    useServersStore().setCurrent("forge");
    const wrapper = mount(AccountTable, {
      props: {
        accounts: [
          account({ username: "marie", role: "admin" }),
          account({ username: "paul", role: "unknown" }),
        ],
        meId: "MARIE",
        rightsKnown: false,
      },
      global: { plugins: [pinia] },
      attachTo: document.body,
    });
    const row = wrapper.get('[data-account="paul"]');
    expect(row.text()).toContain("Rôle inconnu de ce client");
    const change = row
      .findAll("button")
      .find((button) => button.attributes("aria-label")?.startsWith("Changer le rôle"));
    expect(change).toBeDefined();
    expect(change?.attributes("aria-disabled")).toBe("true");
    wrapper.unmount();
  });
});

describe("un réglage de confirmation que ce client ne sait pas lire", () => {
  it("n'est ni coché ni modifiable à l'aveugle", async () => {
    const { pinia, bridge } = await startedApp();
    bridge.reauth.setMode("forge", "unknown");
    const wrapper = mount(ReauthSettingCard, {
      props: { serverId: "forge" },
      global: { plugins: [pinia] },
      attachTo: document.body,
    });
    await flushPromises();
    expect(wrapper.get("[data-reauth-unknown]").text()).toContain("ne sait pas lire ce réglage");
    expect(wrapper.find('[role="radio"]').exists()).toBe(false);
    wrapper.unmount();
  });
});
