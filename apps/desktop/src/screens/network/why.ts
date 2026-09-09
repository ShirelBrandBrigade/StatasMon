import type { useCapability } from "../../hooks/useDevice";

type Cap = ReturnType<typeof useCapability>;

/** Why a value is missing: the capability's reason, or the honest interim states. */
export function why(cap: Cap): string {
  if (cap.available === false) return cap.reason ?? "Not available on this device";
  if (cap.available === undefined) return "Waiting for the device's capability list";
  return "Not reported yet";
}

/** Not in the data contract at all (proposal in the report), regardless of the platform. */
export const NOT_IN_CONTRACT = "Not in the data contract yet";
