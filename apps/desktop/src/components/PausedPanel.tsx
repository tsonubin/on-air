import { Button } from "../ui/Button";

/** Covers the routing and mixer area while the audio service is off. */
export function PausedPanel({
  paused,
  canResume,
  resuming,
  onResume,
}: {
  paused: boolean;
  canResume: boolean;
  resuming: boolean;
  onResume(): void;
}) {
  if (!paused) return null;
  return (
    <section
      aria-labelledby="paused-title"
      data-testid="paused-panel"
      className="absolute inset-0 z-10 flex items-center justify-center bg-scrim p-4"
    >
      <div className="panel-raised flex max-w-[360px] flex-col gap-3 rounded-md border border-line p-4 text-center">
        <h2 id="paused-title" className="m-0 font-sign text-lg font-normal tracking-[0.06em]">
          Audio service is off
        </h2>
        <p className="m-0 text-sm text-steel">
          Capture and playback are stopped and this computer may sleep. Your source, speaker and
          mixer settings come back when the service is turned on.
        </p>
        {canResume ? (
          <Button
            variant="primary"
            data-testid="resume-service"
            onClick={onResume}
            disabled={resuming}
            aria-busy={resuming}
          >
            {resuming ? "Turning on…" : "Turn service on"}
          </Button>
        ) : (
          <p className="m-0 text-sm text-steel-dim">Turn it on from the on-air tray menu.</p>
        )}
      </div>
    </section>
  );
}
