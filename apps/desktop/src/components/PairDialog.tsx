import * as Dialog from "@radix-ui/react-dialog";
import { type FormEvent, useEffect, useRef, useState } from "react";
import type { PairTarget } from "../hooks/useOutputSelection";
import { Button } from "../ui/Button";

export interface PairDialogProps {
  target: PairTarget | null;
  pairing: boolean;
  onSubmit(pin: string): void;
  onCancel(): void;
}

/**
 * Radix Dialog gives the modal semantics: focus trap, Escape and
 * `aria-hidden` on the rest of the page; focus goes back to whatever was
 * focused when it opened. The body is a `<form>` so Enter submits.
 */
export function PairDialog({ target, pairing, onSubmit, onCancel }: PairDialogProps) {
  const [pin, setPin] = useState("");
  const pinInput = useRef<HTMLInputElement>(null);
  const submitButton = useRef<HTMLButtonElement>(null);
  // Opened programmatically (no Dialog.Trigger), so Radix has nothing to
  // restore focus to; remember the opener ourselves.
  const opener = useRef<HTMLElement | null>(null);
  const needsPin = target?.transport === "airplay";

  // A new target starts with an empty PIN.
  const targetKey = target ? `${target.transport}-${target.id}` : "";
  useEffect(() => {
    if (targetKey) setPin("");
  }, [targetKey]);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (pairing) return;
    onSubmit(pin.trim());
  };

  return (
    <Dialog.Root
      open={target !== null}
      onOpenChange={(open) => {
        if (!open && !pairing) onCancel();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-20 bg-scrim" />
        <Dialog.Content
          aria-modal="true"
          className="fixed inset-x-0 bottom-0 z-30 mx-auto w-[calc(100%-2rem)] max-w-[420px] rounded-t-[10px] rounded-b border border-line bg-face-2 p-4 shadow-[0_-12px_40px_var(--color-shadow)] mb-4"
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            opener.current =
              document.activeElement instanceof HTMLElement ? document.activeElement : null;
            (needsPin ? pinInput.current : submitButton.current)?.focus();
          }}
          onCloseAutoFocus={(e) => {
            e.preventDefault();
            if (opener.current?.isConnected) opener.current.focus();
            opener.current = null;
          }}
          onEscapeKeyDown={(e) => {
            if (pairing) e.preventDefault();
          }}
          onInteractOutside={(e) => {
            if (pairing) e.preventDefault();
          }}
        >
          <form onSubmit={submit}>
            <Dialog.Title className="mt-0 mb-1.5 font-sign text-lg font-normal">
              Pair {target?.name}
            </Dialog.Title>
            <Dialog.Description className="mt-0 mb-3 text-sm text-steel">
              {needsPin
                ? "Only if this speaker shows a code (Home app or Apple TV). HomePod mini has no screen and usually has no PIN."
                : "Confirm pairing on the speaker or in this computer’s Bluetooth settings, then continue."}
            </Dialog.Description>
            {needsPin && (
              <label className="flex flex-col gap-1 font-mono text-xs tracking-[0.14em] text-steel-dim uppercase">
                Speaker PIN
                <input
                  ref={pinInput}
                  name="pin"
                  type="text"
                  inputMode="numeric"
                  autoComplete="one-time-code"
                  placeholder="••••"
                  value={pin}
                  disabled={pairing}
                  onChange={(e) => setPin(e.target.value)}
                  className="h-auto w-full rounded border border-line bg-well p-2.5 font-mono text-lg normal-case tracking-[0.28em] text-ink focus-visible:border-amber focus-visible:outline-none"
                />
              </label>
            )}
            <div className="mt-3 flex gap-2">
              <Button variant="secondary" disabled={pairing} onClick={onCancel}>
                Cancel
              </Button>
              <Button
                ref={submitButton}
                variant="primary"
                type="submit"
                disabled={pairing}
                aria-busy={pairing}
              >
                {pairing ? "Pairing…" : "Pair & go live"}
              </Button>
            </div>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
