// Lets `npm run dev` be opened directly in a regular browser (no Tauri
// shell) for fast UI iteration, by mocking the handful of IPC commands
// the app calls. Only active in dev builds, and only when there's no
// real Tauri context already — `tauri dev`/`tauri build` never load this.
import { mockIPC } from "@tauri-apps/api/mocks";
import type { Environment } from "@/bindings/Environment";
import type { SystemCheck } from "@/bindings/SystemCheck";

const MOCK_ENVIRONMENTS: Environment[] = [
  { chain: "mainnet", name: "Mainnet", rpc_port: 8332, p2p_port: 8333, ord_port: 8080, data_root: "data/mainnet" },
  { chain: "regtest", name: "Regtest", rpc_port: 18443, p2p_port: 18444, ord_port: 8081, data_root: "data/regtest" },
  { chain: "signet", name: "Signet", rpc_port: 38332, p2p_port: 38333, ord_port: 8082, data_root: "data/signet" },
  {
    chain: "testnet4",
    name: "Testnet4",
    rpc_port: 48332,
    p2p_port: 48333,
    ord_port: 8083,
    data_root: "data/testnet4",
  },
];

const MOCK_SYSTEM_CHECK: SystemCheck = {
  os: "windows",
  arch: "x86_64",
  cpu_cores: 8,
  total_memory_bytes: 17_179_869_184,
  available_memory_bytes: 8_589_934_592,
  disk_free_bytes: 256_060_514_304,
};

const settings = new Map<string, string>();

export function installDevTauriMockIfNeeded() {
  if (!import.meta.env.DEV || "__TAURI_INTERNALS__" in window) return;

  mockIPC((cmd, args) => {
    switch (cmd) {
      case "list_default_environments":
        return MOCK_ENVIRONMENTS;
      case "system_check":
        return MOCK_SYSTEM_CHECK;
      case "get_setting":
        return settings.get((args as { key: string }).key) ?? null;
      case "set_setting": {
        const { key, value } = args as { key: string; value: string };
        settings.set(key, value);
        return undefined;
      }
      default:
        throw new Error(`dev-tauri-mock: no mock for IPC command "${cmd}"`);
    }
  });
}
