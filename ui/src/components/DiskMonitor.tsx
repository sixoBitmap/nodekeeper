import { useTranslation } from "react-i18next";
import type { DiskUsage } from "@/bindings/DiskUsage";
import { formatBytes } from "@/lib/format";
import { StatusBadge } from "@/components/StatusBadge";

// docs/SPEC.md item 2: "warn well before free space gets low (Bitcoin
// Core shuts down when the disk is nearly full)." A conservative fixed
// threshold until settings makes this configurable per environment.
export const LOW_SPACE_WARNING_BYTES = 5 * 1024 ** 3; // 5 GiB

export function DiskMonitor({ disk }: { disk: DiskUsage }) {
  const { t } = useTranslation();
  const isLow =
    disk.free_on_volume_bytes !== null && disk.free_on_volume_bytes < LOW_SPACE_WARNING_BYTES;

  return (
    <div className="space-y-1 rounded-lg border border-border p-3 text-sm">
      <div className="flex items-center justify-between">
        <span className="font-medium">{t("diskMonitor.title")}</span>
        {isLow && <StatusBadge label={t("diskMonitor.lowSpace")} variant="danger" />}
      </div>
      <dl className="grid grid-cols-2 gap-y-1 text-muted-foreground">
        <dt>{t("diskMonitor.used")}</dt>
        <dd className="text-right text-foreground">{formatBytes(disk.used_by_data_bytes)}</dd>
        <dt>{t("diskMonitor.free")}</dt>
        <dd className="text-right text-foreground">
          {disk.free_on_volume_bytes !== null
            ? formatBytes(disk.free_on_volume_bytes)
            : t("diskMonitor.freeUnknown")}
        </dd>
      </dl>
    </div>
  );
}
