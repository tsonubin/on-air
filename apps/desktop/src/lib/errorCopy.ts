import { isServicePaused, errorCopy as sharedErrorCopy } from "@on-air/control-client";

export { isServicePaused };

export const UNREACHABLE_COPY = "Cannot reach the audio service on this computer.";
/** Shown when the core reports that the active source stopped on its own. */
export const INPUT_STOPPED_COPY = "The source stopped unexpectedly. Choose it again to restart.";

/** The shared sentences, with the ones that name this window's situation. */
export function errorCopy(err: unknown): string {
  return sharedErrorCopy(err, {
    codes: { not_paired: "This window is not paired with the audio service. Restart on-air." },
    unreachable: UNREACHABLE_COPY,
  });
}
