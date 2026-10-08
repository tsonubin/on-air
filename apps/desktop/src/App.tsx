import { DeviceHelp } from "./components/DeviceHelp";
import { DeviceList } from "./components/DeviceList";
import { ErrorBanner } from "./components/ErrorBanner";
import { Header } from "./components/Header";
import { MixerFooter } from "./components/MixerFooter";
import { PairDialog } from "./components/PairDialog";
import { PausedPanel } from "./components/PausedPanel";
import { useAutostart } from "./hooks/useAutostart";
import { useCdTransport } from "./hooks/useCdTransport";
import { useCoreSnapshot } from "./hooks/useCoreSnapshot";
import { useMixer } from "./hooks/useMixer";
import { useOutputSelection } from "./hooks/useOutputSelection";
import { useService } from "./hooks/useService";
import { UNREACHABLE_COPY } from "./lib/errorCopy";
import { CdTransport } from "./ui/CdTransport";

function App() {
  const core = useCoreSnapshot();
  const { snapshot, connection } = core;
  const service = useService(core);
  const selection = useOutputSelection({ core });
  const mixer = useMixer({ snapshot, reportError: core.reportError, refresh: core.refresh });
  const cd = useCdTransport(core);
  const { autostart, toggle: toggleAutostart } = useAutostart(core.reportError);
  const locked = service.paused;

  return (
    <main className="flex h-full min-h-full flex-col gap-2.5 overflow-auto p-[clamp(10px,2vw,18px)]">
      <div className="chassis flex min-h-0 flex-1 flex-col rounded-[10px] border border-line">
        <Header
          connection={connection}
          liveUpdates={core.liveUpdates}
          status={snapshot.status}
          activeOutput={snapshot.activeOutput}
          pin={snapshot.pin}
          airplayMode={snapshot.airplayMode}
          autostart={autostart}
          onToggleAutostart={() => void toggleAutostart()}
        />
        {connection === "unreachable" && (
          <p className="mx-3 mt-2 mb-0 text-sm text-amber">{UNREACHABLE_COPY} Retrying…</p>
        )}
        <ErrorBanner error={core.actionError} onDismiss={core.clearError} />
        <CdTransport
          status={snapshot.cd}
          disabled={locked}
          onPlayPause={cd.playPause}
          onPrev={cd.prev}
          onNext={cd.next}
        />
        <DeviceHelp
          topic={selection.deviceHelp}
          refreshing={selection.refreshingDevices}
          onRefresh={() => void selection.refreshDevices()}
          onOpenPicker={() => void selection.openAirplayPicker()}
          onDismiss={selection.dismissHelp}
        />
        <div className="relative flex min-h-0 flex-1 flex-col">
          <div className="flex min-h-0 flex-1 flex-col" inert={locked || undefined}>
            <DeviceList
              inputs={snapshot.inputs}
              activeInput={snapshot.activeInput}
              outputs={snapshot.outputs}
              activeOutput={snapshot.activeOutput}
              airplayMode={snapshot.airplayMode}
              connectingOutput={selection.connectingOutput}
              refreshing={selection.refreshingDevices}
              disabled={locked}
              onChooseInput={(name) => void selection.chooseInput(name)}
              onChooseOutput={(output) => void selection.chooseOutput(output)}
              onBluetoothSettings={selection.openBluetoothSettings}
              onAirplayInfo={selection.showAirplayHelp}
              onRefresh={() => void selection.refreshDevices()}
            />
            <MixerFooter mixer={mixer} disabled={locked} />
          </div>
          <PausedPanel
            paused={locked}
            canResume={service.canResume}
            resuming={service.resuming}
            onResume={() => void service.resume()}
          />
        </div>
      </div>
      <PairDialog
        target={selection.pairTarget}
        pairing={selection.pairing}
        onSubmit={(pin) => void selection.submitPair(pin)}
        onCancel={selection.cancelPair}
      />
    </main>
  );
}

export default App;
