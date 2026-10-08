/** Amber segment display with an unlit "8" ghost behind the digits. */
export function LedReadout({
  ghost,
  testId,
  children,
}: {
  ghost: string;
  testId?: string;
  children: React.ReactNode;
}) {
  return (
    <span className="led-readout relative inline-grid rounded-sm px-1.5 py-0.5 text-base tracking-[0.1em]">
      <span className="col-start-1 row-start-1 text-led-ghost select-none" aria-hidden="true">
        {ghost}
      </span>
      <span className="led-glow col-start-1 row-start-1 text-amber" data-testid={testId}>
        {children}
      </span>
    </span>
  );
}
