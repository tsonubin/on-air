import { FieldGroup, Host, Text } from "@expo/ui";
import type { OutputInfo } from "@on-air/api-types";
import { router } from "expo-router";
import { useState } from "react";
import { View } from "react-native";
import { MixerHome } from "@/mixer-home";
import { PairingHome } from "@/pairing-home";
import { type DevicePairTarget, useRemoteSession } from "@/remote-session";
import { DevicePairSheet } from "@/sheets/device-pair-sheet";
import { OutputSheet } from "@/sheets/output-sheet";
import { SourceSheet } from "@/sheets/source-sheet";
import { colors } from "@/theme";

function RestoringScreen() {
  return (
    <Host style={{ flex: 1 }} colorScheme="dark">
      <FieldGroup testID="pairing-restore">
        <FieldGroup.Section title="on-air remote">
          <Text>Restoring your desktop connection…</Text>
        </FieldGroup.Section>
      </FieldGroup>
    </Host>
  );
}

function PairingScreen() {
  const remote = useRemoteSession();
  return (
    <PairingHome
      scanning={remote.scanning}
      pairing={remote.pairing}
      found={remote.found}
      host={remote.hostInput}
      pin={remote.pin}
      error={remote.error}
      onScan={remote.scan}
      onSelectHost={remote.selectHost}
      onChangeHost={remote.changeHost}
      onChangePin={remote.changePin}
      onPair={() => void remote.pair()}
    />
  );
}

/** Mounted only while paired, so sheet state resets when the pairing ends. */
function MixerScreen() {
  const remote = useRemoteSession();
  const [sheet, setSheet] = useState<"source" | "output" | null>(null);
  const [pairTarget, setPairTarget] = useState<DevicePairTarget | null>(null);
  const { cd, view } = remote;

  const requestPair = (output: OutputInfo) => {
    setSheet(null);
    setPairTarget({ transport: output.transport, id: output.id, name: output.name });
  };

  return (
    <>
      <MixerHome
        activeInput={remote.activeInputLabel}
        activeOutput={remote.activeOutput?.device_name ?? ""}
        volume={remote.volume}
        status={{ text: view.statusText, announcement: view.statusAnnouncement, live: view.live }}
        error={remote.error ?? remote.warning}
        volumeDisabled={!view.volumeEnabled}
        soundDisabled={!view.soundEnabled}
        cd={cd}
        onChangeInput={() => setSheet("source")}
        onChangeOutput={() => setSheet("output")}
        onOpenSound={() => router.push("/sound")}
        onOpenMore={() => router.push("/connection")}
        onDisconnect={remote.disconnect}
        onVolumeChange={remote.applyVolume}
        onCdPlayPause={() => void remote.controlCd(cd.playing ? "pause" : "play")}
        onCdPrev={() => void remote.controlCd("prev")}
        onCdNext={() => void remote.controlCd("next")}
      />
      {sheet === "source" && <SourceSheet onClose={() => setSheet(null)} />}
      {sheet === "output" && (
        <OutputSheet onClose={() => setSheet(null)} onPairRequest={requestPair} />
      )}
      {pairTarget && <DevicePairSheet target={pairTarget} onClose={() => setPairTarget(null)} />}
    </>
  );
}

export default function HomeScreen() {
  const { uiState } = useRemoteSession();
  return (
    <View style={{ flex: 1, backgroundColor: colors.background }}>
      {uiState === "restoring" ? (
        <RestoringScreen />
      ) : uiState === "pairing" ? (
        <PairingScreen />
      ) : (
        <MixerScreen />
      )}
    </View>
  );
}
