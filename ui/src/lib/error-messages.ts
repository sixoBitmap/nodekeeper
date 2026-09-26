import i18n from "@/i18n";
import type { AppErrorCode } from "@/bindings/AppErrorCode";
import type { TypedError } from "@/bindings/TypedError";

export interface FriendlyError {
  title: string;
  message: string;
  whatToDo?: string;
}

// docs/SPEC.md item 8: "Plain-language errors: map common failures ...
// to friendly messages with a What to do button." The backend
// (nk_core::AppErrorCode) only carries the code — this is the one place
// that maps it to actual words, so every error the user sees is worded
// consistently and translatably.
const CODE_TO_I18N_KEY: Record<AppErrorCode, string> = {
  PORT_IN_USE: "portInUse",
  DISK_FULL: "diskFull",
  INDEX_BEHIND: "indexBehind",
  INDEX_OPTION_DISABLED: "indexOptionDisabled",
  WALLET_LOCKED: "walletLocked",
  RPC_WARMING_UP: "rpcWarmingUp",
  ORD_NOT_SYNCED: "ordNotSynced",
  BINARY_NOT_VERIFIED: "binaryNotVerified",
  WALLET_NOT_ENCRYPTED: "walletNotEncrypted",
};

/** Turns a `TypedError` from a Tauri command into UI-ready text. */
export function friendlyError(error: TypedError): FriendlyError {
  if (error.code) {
    const key = CODE_TO_I18N_KEY[error.code];
    return {
      title: i18n.t(`errors.${key}.title`),
      message: i18n.t(`errors.${key}.message`),
      whatToDo: i18n.t(`errors.${key}.whatToDo`),
    };
  }
  // No code: still show *something* useful rather than a blank panel —
  // the raw message is always available as the technical detail.
  return {
    title: i18n.t("errors.generic.title"),
    message: i18n.t("errors.generic.message"),
  };
}
