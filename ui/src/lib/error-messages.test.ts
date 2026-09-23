import { describe, expect, it } from "vitest";
import "@/i18n";
import { friendlyError } from "./error-messages";

describe("friendlyError", () => {
  it("maps every AppErrorCode to non-empty title and message text", () => {
    const codes = [
      "PORT_IN_USE",
      "DISK_FULL",
      "INDEX_BEHIND",
      "INDEX_OPTION_DISABLED",
      "WALLET_LOCKED",
      "RPC_WARMING_UP",
      "ORD_NOT_SYNCED",
      "BINARY_NOT_VERIFIED",
    ] as const;

    for (const code of codes) {
      const result = friendlyError({ code, message: "technical detail" });
      expect(result.title).not.toBe("");
      expect(result.message).not.toBe("");
      // Every code above has real "what to do" guidance defined.
      expect(result.whatToDo).toBeTruthy();
    }
  });

  it("falls back to a generic message when there is no code", () => {
    const result = friendlyError({ code: null, message: "raw technical detail" });
    expect(result.title).not.toBe("");
    expect(result.message).not.toBe("");
  });
});
