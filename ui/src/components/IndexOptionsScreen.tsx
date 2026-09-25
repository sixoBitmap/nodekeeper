import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { Environment } from "@/bindings/Environment";
import type { IndexOptions } from "@/bindings/IndexOptions";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";

const INDEX_OPTIONS_SETTING_PREFIX = "index_options_";
const CHAIN_DIR_NAME: Record<Chain, string> = {
  mainnet: "mainnet",
  regtest: "regtest",
  signet: "signet",
  testnet4: "testnet4",
};

/**
 * docs/SPEC.md Foundation F: "Each environment records its ord index
 * options ... state clearly that these choices are effectively
 * permanent; changing them later means a full reindex." Shown once,
 * after Binary Setup -- skips itself once every chain already has a
 * saved choice (the settings themselves are the "already done" marker,
 * same pattern as `BinarySetupScreen`).
 */
export function IndexOptionsScreen({ onContinue }: { onContinue: () => void }) {
  const { t } = useTranslation();
  const [environments, setEnvironments] = useState<Environment[] | null>(null);
  const [choices, setChoices] = useState<Record<Chain, IndexOptions> | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void Promise.all([
      invoke<Environment[]>("list_default_environments"),
      Promise.all(
        (Object.keys(CHAIN_DIR_NAME) as Chain[]).map((chain) =>
          invoke<string | null>("get_setting", {
            key: `${INDEX_OPTIONS_SETTING_PREFIX}${CHAIN_DIR_NAME[chain]}`,
          }),
        ),
      ),
    ]).then(([envs, savedFlags]) => {
      if (cancelled) return;
      if (savedFlags.every((v) => v !== null)) {
        onContinue();
        return;
      }
      setEnvironments(envs);
      const initial = {} as Record<Chain, IndexOptions>;
      for (const env of envs) initial[env.chain] = env.index_options;
      setChoices(initial);
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- onContinue is stable for the lifetime of this screen
  }, []);

  const toggle = (chain: Chain, key: keyof IndexOptions) => {
    setChoices((prev) => {
      if (!prev) return prev;
      return { ...prev, [chain]: { ...prev[chain], [key]: !prev[chain][key] } };
    });
  };

  const save = async () => {
    if (!choices) return;
    setSaving(true);
    try {
      for (const chain of Object.keys(CHAIN_DIR_NAME) as Chain[]) {
        const o = choices[chain];
        await invoke("set_index_options", {
          chain,
          indexSats: o.index_sats,
          indexRunes: o.index_runes,
          indexAddresses: o.index_addresses,
        });
      }
      onContinue();
    } finally {
      setSaving(false);
    }
  };

  if (!environments || !choices) {
    return <div className="h-full bg-background" />;
  }

  return (
    <div className="flex h-full flex-col items-center justify-center gap-6 bg-background p-8 text-foreground">
      <div className="w-full max-w-lg space-y-4">
        <div>
          <h1 className="text-2xl font-semibold">{t("indexOptions.title")}</h1>
          <p className="mt-1 text-sm text-muted-foreground">{t("indexOptions.intro")}</p>
        </div>

        <div className="space-y-3">
          {environments.map((env) => (
            <div key={env.chain} className="space-y-2 rounded-md border border-border p-3">
              <h2 className="text-sm font-medium">{env.name}</h2>
              <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
                <OptionToggle
                  label={t("indexOptions.sats")}
                  hint={t("indexOptions.satsHint")}
                  checked={choices[env.chain].index_sats}
                  onChange={() => toggle(env.chain, "index_sats")}
                />
                <OptionToggle
                  label={t("indexOptions.runes")}
                  hint={t("indexOptions.runesHint")}
                  checked={choices[env.chain].index_runes}
                  onChange={() => toggle(env.chain, "index_runes")}
                />
                <OptionToggle
                  label={t("indexOptions.addresses")}
                  hint={t("indexOptions.addressesHint")}
                  checked={choices[env.chain].index_addresses}
                  onChange={() => toggle(env.chain, "index_addresses")}
                />
              </div>
            </div>
          ))}
        </div>

        <p className="text-xs text-muted-foreground">{t("indexOptions.permanenceNote")}</p>

        <Button className="w-full" disabled={saving} onClick={() => void save()}>
          {t("systemCheck.continue")}
        </Button>
      </div>
    </div>
  );
}

function OptionToggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: () => void;
}) {
  return (
    <label className="flex items-start gap-2 text-xs">
      <Checkbox checked={checked} onCheckedChange={onChange} className="mt-0.5" />
      <span>
        <span className="block font-medium text-foreground">{label}</span>
        <span className="block text-muted-foreground">{hint}</span>
      </span>
    </label>
  );
}
