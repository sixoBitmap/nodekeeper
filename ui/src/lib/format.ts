/** Human-readable byte size, e.g. `formatBytes(1_500_000_000) === "1.4 GB"`. */
export function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** exponent;
  return `${value.toFixed(exponent === 0 ? 0 : 1)} ${units[exponent]}`;
}

/** Human-readable duration, e.g. `formatUptime(3665) === "1h 1m"`. */
export function formatUptime(seconds: number): string {
  if (seconds < 60) return `${Math.floor(seconds)}s`;
  const totalMinutes = Math.floor(seconds / 60);
  const days = Math.floor(totalMinutes / (60 * 24));
  const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
  const minutes = totalMinutes % 60;
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

/** Sats as a BTC amount, e.g. `formatSats(100_000_000) === "1.00000000 BTC"`. */
export function formatSats(sats: number): string {
  return `${(sats / 100_000_000).toFixed(8)} BTC`;
}
