import { FieldGroup } from "@expo/ui";
import type { OutputInfo } from "@on-air/api-types";
import { outputPhase } from "@on-air/api-types";
import { router } from "expo-router";
import { Stack } from "expo-router/stack";
import { useState } from "react";
import { Platform, View } from "react-native";
import { EqPane } from "@/eq-panel";
import { MixerHome } from "@/mixer-home";
import { PairingHome } from "@/pairing-home";
import { type DevicePairTarget, useRemoteSession } from "@/remote-session";
import { SettingsForm, sectionModifiers } from "@/settings-form";
import { DevicePairSheet } from "@/sheets/device-pair-sheet";
import { OutputSheet } from "@/sheets/output-sheet";
import { SourceSheet } from "@/sheets/source-sheet";
import { colors } from "@/theme";
import { FormText, sectionTitle } from "@/ui/form-text";
import { homeStatus } from "@/ui-state";

function RestoringScreen() {
  return (
    <SettingsForm testID="pairing-restore">
      <FieldGroup.Section modifiers={sectionModifiers}>
        {sectionTitle("on-air remote")}
        <FormText variant="body">Restoring your desktop connection…</FormText>
      </FieldGroup.Section>
    </SettingsForm>
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

  const openMore = () => router.push("/connection");
  return (
    <>
      {Platform.OS === "ios" ? (
        // More options lives in the native toolbar, which the folded iPhone Duo moves into its
        // side rail. The header is clear (no title, no fill) so the plate and the sign show through.
        <>
          <Stack.Screen
            options={{
              headerShown: true,
              headerTransparent: true,
              headerTitle: "",
              headerStyle: { backgroundColor: "transparent" },
            }}
          />
          <Stack.Toolbar placement="right">
            <Stack.Toolbar.Button
              icon="ellipsis"
              accessibilityLabel="More options"
              onPress={openMore}
            >
              More options
            </Stack.Toolbar.Button>
          </Stack.Toolbar>
        </>
      ) : null}
      <MixerHome
        activeInput={remote.activeInputLabel}
        activeOutput={remote.activeOutput?.device_name ?? ""}
        outputPhase={remote.activeOutput ? outputPhase(remote.activeOutput) : undefined}
        volume={remote.volume}
        status={homeStatus({
          uiState: remote.uiState,
          outputPhase: remote.activeOutput ? outputPhase(remote.activeOutput) : undefined,
          firewallBlocked: remote.firewallWarning != null,
        })}
        outputSampleRate={remote.outputSampleRate}
        error={remote.error ?? remote.warning}
        firewallWarning={remote.firewallWarning}
        volumeDisabled={!view.volumeEnabled}
        soundDisabled={!view.soundEnabled}
        cd={cd}
        onChangeInput={() => setSheet("source")}
        onChangeOutput={() => setSheet("output")}
        eqFlat={remote.gains.every((g) => g === 0)}
        onOpenEqualizer={() => router.push("/equalizer")}
        onOpenFormat={() => router.push("/audio-format")}
        onOpenMore={Platform.OS === "ios" ? undefined : openMore}
        onVolumeChange={remote.applyVolume}
        onCdPlayPause={() => void remote.controlCd(cd.playing ? "pause" : "play")}
        onCdPrev={() => void remote.controlCd("prev")}
        onCdNext={() => void remote.controlCd("next")}
        eqPane={
          <EqPane
            gains={remote.gains}
            disabled={!view.controlsEnabled}
            onChangeBand={(index, value) => remote.changeBand(index, value)}
            onReset={() => remote.applyEq([0, 0, 0, 0, 0])}
          />
        }
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
