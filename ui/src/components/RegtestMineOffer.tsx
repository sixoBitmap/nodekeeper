import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { Chain } from "@/bindings/Chain";
import type { TypedError } from "@/bindings/TypedError";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";

const AUTO_MINE_SETTING_KEY = "regtest_auto_mine";

/**
 * docs/SPEC.md item 11: "Post-action 'Mine 1 block to confirm' offer
 * after a send/inscribe on regtest, plus an optional auto-mine
 * toggle." A Regtest send/inscribe sits unconfirmed forever unless
 * someone mines a block, so every send/inscribe success view mounts
 * this right after its result. Renders nothing on any other chain --
 * callers can mount it unconditionally rather than each repeating the
 * `chain === "regtest"` check.
 *
 * The auto-mine choice is a single global setting (not per-chain --
 * only Regtest ever self-mines) so that once a user turns it on, every
 * later action across every form picks it up automatically, including
 * the one this instance mounted for.
 */
export function RegtestMineOffer({ chain }: { chain: Chain }) {
  const { t } = useTranslation();
  const [autoMine, setAutoMine] = useState(false);
  const [settingLoaded, setSettingLoaded] = useState(false);
  const [mining, setMining] = useState(false);
  const [mined, setMined] = useState(false);
  const [error, setError] = useState<TypedError | null>(null);

  const mine = () => {
    setMining(true);
    setError(null);
    invoke("mine_blocks", { chain, count: 1 })
      .then(() => setMined(true))
      .catch((e: TypedError) => setError(e))
      .finally(() => setMining(false));
  };

  useEffect(() => {
    if (chain !== "regtest") return;
    invoke<string | null>("get_setting", { key: AUTO_MINE_SETTING_KEY }).then((value) => {
      setSettingLoaded(true);
      if (value === "true") {
        setAutoMine(true);
        mine();
      }
    });
    // Runs once for the lifetime of this success view -- a fresh
    // instance mounts for every new action, so there's no case where
    // this needs to re-run for the same mount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chain]);

  if (chain !== "regtest") return null;

  const toggleAutoMine = (checked: boolean) => {
    setAutoMine(checked);
    void invoke("set_setting", { key: AUTO_MINE_SETTING_KEY, value: String(checked) });
    if (checked && !mined && !mining) mine();
  };

  return (
    <div className="space-y-2 rounded-md border border-dashed border-border p-3 text-sm">
      <p className="text-muted-foreground">{t("regtestMineOffer.hint")}</p>

      {error && <p className="text-xs text-danger">{error.message}</p>}

      {mined ? (
        <p className="text-xs text-success">{t("regtestMineOffer.mined")}</p>
      ) : (
        <Button size="sm" variant="outline" disabled={mining || !settingLoaded} onClick={mine}>
          {mining ? t("regtestMineOffer.mining") : t("regtestMineOffer.mineNow")}
        </Button>
      )}

      <label className="flex items-center gap-2 text-xs text-muted-foreground">
        <Checkbox checked={autoMine} onCheckedChange={(v) => toggleAutoMine(v === true)} />
        {t("regtestMineOffer.autoMine")}
      </label>
    </div>
  );
}
