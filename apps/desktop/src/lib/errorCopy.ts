import { HttpError, isServicePaused, errorCopy as sharedErrorCopy } from "@on-air/control-client";

export { isServicePaused };

export const UNREACHABLE_COPY = "Cannot reach the audio service on this computer.";
/** Shown when the core reports that the active source stopped on its own. */
export const INPUT_STOPPED_COPY = "The source stopped unexpectedly. Choose it again to restart.";

/**
 * A 400 rejection carries the core's own guidance for the exact case
 * ("Mono Speaker plays mono only; stereo would drop the right channel. Choose
 * Auto or Mono."), which no shared sentence can express, so it reaches the
 * banner verbatim. Other codes keep the shared copy.
 */
function coreExplanation(err: unknown): string | undefined {
  if (!(err instanceof HttpError) || err.code !== "invalid_request") return undefined;
  try {
    const parsed: unknown = JSON.parse(err.body);
    const message = (parsed as { error?: unknown } | null)?.error;
    return typeof message === "string" && message ? message : undefined;
  } catch {
    return undefined;
  }
}

/** The shared sentences, with the ones that name this window's situation. */
export function errorCopy(err: unknown): string {
  return (
    coreExplanation(err) ??
    sharedErrorCopy(err, {
      codes: { not_paired: "This window is not paired with the audio service. Restart on-air." },
      unreachable: UNREACHABLE_COPY,
    })
  );
}
