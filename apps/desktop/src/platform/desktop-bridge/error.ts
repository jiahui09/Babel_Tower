export type BridgeErrorCode =
  | "ipc_unavailable"
  | "not_implemented"
  | "not_found"
  | "revision_conflict"
  | "validation_failed"
  | "permission_denied"
  | "invalid_request"
  | "internal";

export class BridgeError extends Error {
  readonly code: BridgeErrorCode;
  readonly details?: Record<string, unknown>;

  constructor(code: BridgeErrorCode, message: string, details?: Record<string, unknown>) {
    super(message);
    this.name = "BridgeError";
    this.code = code;
    this.details = details;
  }
}

export function normalizeBridgeError(reason: unknown): BridgeError {
  if (reason instanceof BridgeError) return reason;
  if (reason instanceof Error) return new BridgeError("internal", reason.message);
  return new BridgeError("internal", String(reason));
}

