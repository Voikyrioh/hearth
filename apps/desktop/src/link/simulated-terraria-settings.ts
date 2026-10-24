import type { FicheSetting, FicheTheme, SettingLevel } from "./catalog";
import wire from "./fixtures/terraria-settings.json";

/**
 * Le catalogue de réglages de Terraria tel que l'agent le sert (`GET /catalog/terraria`, champs `themes` et `settings`),
 * lu dans `fixtures/terraria-settings.json`. Ce fichier n'est JAMAIS écrit à la main : il est généré depuis la fiche
 * embarquée par le test Rust `terraria_fixture_of_the_simulated_client_follows_the_embedded_fiche`
 * (`crates/hearth-agent/src/application/catalog.rs`), qui échoue quand la fiche bouge sans que la fixture suive
 * (`HEARTH_REGEN_FIXTURES=1 cargo test -p hearth-agent --lib terraria_fixture`). Il porte aussi le canal de chaque réglage
 * (`config` ou `args`), que le simulateur lit pour en déduire l'effet comme l'agent. Jamais livré (derrière
 * `import.meta.env.DEV`).
 */

interface WireQuestion {
  id: string;
  label: string;
  help: string;
  secret: boolean;
  proposed?: string;
  kind: FicheSetting["question"]["kind"];
  choices?: { value: string; label: string }[];
  min?: number;
  max?: number;
  min_len?: number;
  max_len?: number;
  charset?: FicheSetting["question"]["charset"];
  frozen_after_creation?: boolean;
  variants?: string[];
  secret_kind?: FicheSetting["question"]["secretKind"];
  obtain_url?: string;
}

interface WireSetting {
  question: WireQuestion;
  key: string;
  theme: string;
  level: SettingLevel;
  channel: "config" | "args";
  game_default?: string;
  imposed_why?: string;
  since?: string;
  until?: string;
  replaced_by?: string;
  list?: { item_charset: "digits" | "slug" | "unknown"; separator: string; max_items: number };
}

const fixture = wire as unknown as {
  fiche: { id: string; version: number; digest: string };
  themes: FicheTheme[];
  settings: WireSetting[];
};

function toSetting(item: WireSetting): FicheSetting {
  const q = item.question;
  return {
    question: {
      id: q.id,
      label: q.label,
      help: q.help,
      secret: q.secret,
      proposed: q.proposed ?? null,
      kind: q.kind,
      choices: q.choices ?? [],
      min: q.min ?? null,
      max: q.max ?? null,
      minLen: q.min_len ?? null,
      maxLen: q.max_len ?? null,
      charset: q.charset ?? null,
      frozenAfterCreation: q.frozen_after_creation ?? false,
      variants: q.variants ?? [],
      secretKind: q.secret_kind ?? null,
      obtainUrl: q.obtain_url ?? null,
    },
    key: item.key,
    theme: item.theme,
    level: item.level,
    gameDefault: item.game_default ?? null,
    imposedWhy: item.imposed_why ?? null,
    since: item.since ?? null,
    until: item.until ?? null,
    replacedBy: item.replaced_by ?? null,
    list: item.list
      ? {
          itemCharset: item.list.item_charset,
          separator: item.list.separator,
          maxItems: item.list.max_items,
        }
      : null,
  };
}

/** L'empreinte de la fiche de la fixture : celle de `GET /catalog/terraria` de la construction qui l'a générée. */
export const TERRARIA_DIGEST = fixture.fiche.digest;
export const TERRARIA_THEMES: FicheTheme[] = fixture.themes.map((theme) => ({ ...theme }));
export const TERRARIA_SETTINGS: FicheSetting[] = fixture.settings.map(toSetting);

/** Par où le jeu reçoit chaque réglage : `config` (le fichier) ou `args` (un argument de lancement). */
export const TERRARIA_CHANNEL: Record<string, "config" | "args"> = Object.fromEntries(
  fixture.settings.map((item) => [item.question.id, item.channel]),
);
