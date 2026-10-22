import type { CdStatus, FirewallWarning } from "@on-air/api-types";
import type React from "react";
import { useState } from "react";
import { ScrollView, Text, useWindowDimensions, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { LedDisplay } from "./led-display";
import { formatRate } from "./readouts";
import { colors, layout, type } from "./theme";
import { Faceplate, Groove } from "./ui/faceplate";
import { Fader } from "./ui/fader";
import { FirewallNotice } from "./ui/firewall-notice";
import { icons } from "./ui/icons";
import { LedReadout } from "./ui/led-readout";
import { Notice } from "./ui/notice";
import { PanelKey } from "./ui/panel-key";
import { Sign } from "./ui/sign";
import type { HomeStatus } from "./ui-state";

/** The ONAIR sign is the only status badge up top; the LED window carries the words. */
function Header({ live, onMore }: { live: boolean; onMore: () => void }) {
  const insets = useSafeAreaInsets();
  return (
    <View style={{ paddingTop: insets.top + 10, paddingBottom: 14 }}>
      <View
        style={{
          width: "100%",
          maxWidth: layout.maxWidth,
          alignSelf: "center",
          paddingHorizontal: 20,
          flexDirection: "row",
          alignItems: "center",
        }}
      >
        <View style={{ flex: 1 }}>
          <Sign lit={live} testID="mixer-sign" />
        </View>
        <PanelKey
          shape="round"
          icon={icons.more}
          accessibilityLabel="More options"
          onPress={onMore}
          testID="more-button"
        />
      </View>
    </View>
  );
}

const KEY_GAP = 8;

/**
 * Whether the four key captions fit side by side in a row this wide. The
 * captions are Plex Mono (0.6 em advance) at 11 pt with 1.76 pt tracking,
 * capped at 1.3× text size, in keys with 8 pt padding each side; the longest
 * is seven letters (SPEAKER, FORMAT and SOURCE are six or seven).
 */
export function keysFitInRow(rowWidth: number, fontScale: number): boolean {
  const scale = Math.min(fontScale, 1.3);
  const caption = 7 * (11 * scale * 0.6 + 1.76);
  const key = (rowWidth - 3 * KEY_GAP) / 4;
  return key >= caption + 2 * 8 + 2;
}

function padTrack(n: number): string {
  return String(n).padStart(2, "0");
}

/** The disc transport: an LED track counter and three faceplate keys. */
function CdDeck({
  status,
  onPlayPause,
  onPrev,
  onNext,
}: {
  status: CdStatus;
  onPlayPause: () => void;
  onPrev: () => void;
  onNext: () => void;
}) {
  const label = status.title || status.album || "Audio CD";
  return (
    <View testID="cd-transport" style={{ gap: 12 }}>
      <View
        accessible
        accessibilityLabel={`Track ${status.track} of ${status.track_count}, ${label}`}
        style={{ flexDirection: "row", alignItems: "center", gap: 10 }}
      >
        <LedReadout
          value={`${padTrack(status.track)}/${padTrack(status.track_count)}`}
          digits={5}
          size={16}
          accessibilityLabel={`Track ${status.track} of ${status.track_count}`}
          testID="cd-track"
        />
        <Text numberOfLines={1} style={[type.title, { flex: 1, color: colors.label }]}>
          {label}
        </Text>
      </View>
      <View style={{ flexDirection: "row", justifyContent: "center", gap: 12 }}>
        <PanelKey
          icon={icons.prev}
          iconSize={22}
          accessibilityLabel="Previous track"
          onPress={onPrev}
          style={{ width: 64 }}
          testID="cd-prev"
        />
        <PanelKey
          icon={status.playing ? icons.pause : icons.play}
          iconSize={26}
          accessibilityLabel={status.playing ? "Pause" : "Play"}
          onPress={onPlayPause}
          style={{ width: 64 }}
          testID="cd-play"
        />
        <PanelKey
          icon={icons.next}
          iconSize={22}
          accessibilityLabel="Next track"
          onPress={onNext}
          style={{ width: 64 }}
          testID="cd-next"
        />
      </View>
    </View>
  );
}

/** A printed scale under the volume fader; decoration, so it never reads aloud or counts. */
function Scale({ marks }: { marks: string[] }) {
  return (
    <View
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      style={{ flexDirection: "row", justifyContent: "space-between", paddingHorizontal: 4 }}
    >
      {marks.map((mark) => (
        <Text
          key={mark}
          accessibilityElementsHidden
          importantForAccessibility="no"
          maxFontSizeMultiplier={1.3}
          style={[type.label, { color: colors.tertiaryLabel }]}
        >
          {mark}
        </Text>
      ))}
    </View>
  );
}

export function MixerHome({
  activeInput,
  activeOutput,
  outputPhase,
  volume,
  status,
  outputSampleRate,
  error,
  firewallWarning,
  volumeDisabled,
  soundDisabled,
  cd,
  eqFlat,
  onChangeInput,
  onChangeOutput,
  onOpenEqualizer,
  onOpenFormat,
  onOpenMore,
  onVolumeChange,
  onCdPlayPause,
  onCdPrev,
  onCdNext,
}: {
  activeInput: string;
  activeOutput: string;
  /** Phase of the active output; absent means live (older desktops). */
  outputPhase?: "starting" | "live" | "failed";
  volume: number;
  /** The LED status (`homeStatus`): a firewall block already overrides live. */
  status: HomeStatus;
  /** The output sample rate in Hz for the window's format corner; 0 when unknown. */
  outputSampleRate: number;
  error?: string | null;
  firewallWarning?: FirewallWarning | null;
  volumeDisabled: boolean;
  soundDisabled: boolean;
  cd?: CdStatus | null;
  /** Every EQ band sits at 0 dB. */
  eqFlat: boolean;
  onChangeInput: () => void;
  onChangeOutput: () => void;
  onOpenEqualizer: () => void;
  onOpenFormat: () => void;
  onOpenMore: () => void;
  onVolumeChange: (value: number) => void;
  onCdPlayPause?: () => void;
  onCdPrev?: () => void;
  onCdNext?: () => void;
}): React.JSX.Element {
  const insets = useSafeAreaInsets();
  const { width, height, fontScale } = useWindowDimensions();
  const compact = width < layout.compactBreakpoint;
  const wide = width >= layout.wideBreakpoint;
  const shortWide = wide && height < 600;
  const gap = shortWide ? 12 : compact ? 18 : 22;
  // The four keys share a row while their captions fit; the row is measured, and
  // estimated from the window until then.
  const [keysWidth, setKeysWidth] = useState(0);
  const keysRow = keysWidth || Math.min(width, layout.maxWidth) - 2 * (compact ? 16 : 20);
  const keyGrid = !keysFitInRow(keysRow, fontScale);
  const keyStyle = { flexGrow: 1, flexBasis: keyGrid ? "48%" : 0 } as const;
  // The big LED follows the fader's draft, so it moves with the finger.
  const [shownVolume, setShownVolume] = useState(volume);
  const speakerFailed = activeOutput !== "" && outputPhase === "failed";

  return (
    <Faceplate testID="mixer-screen">
      <Header live={status.live} onMore={onOpenMore} />
      <ScrollView
        style={{ flex: 1 }}
        contentInsetAdjustmentBehavior="automatic"
        showsVerticalScrollIndicator={false}
        contentContainerStyle={{
          paddingHorizontal: compact ? 16 : 20,
          paddingTop: shortWide ? 4 : 8,
          paddingBottom: Math.max(insets.bottom, 18) + 12,
          width: "100%",
          maxWidth: layout.maxWidth,
          alignSelf: "center",
        }}
      >
        <View
          testID={
            shortWide ? "mixer-landscape-layout" : wide ? "mixer-wide-layout" : "mixer-phone-layout"
          }
          style={{
            width: "100%",
            flexDirection: wide ? "row" : "column",
            alignItems: wide ? "flex-start" : "stretch",
            gap: wide ? 24 : gap,
          }}
        >
          <View style={{ flex: wide ? 1.08 : undefined, gap }}>
            <LedDisplay
              source={activeInput}
              speaker={activeOutput}
              status={status}
              rate={formatRate(outputSampleRate)}
            />
            {error ? <Notice message={error} testID="mixer-error" /> : null}
            {speakerFailed ? (
              <Text testID="speaker-failed" style={[type.body, { color: colors.secondaryLabel }]}>
                Speaker failed. Choose it again.
              </Text>
            ) : null}
            <FirewallNotice warning={firewallWarning} />
            {cd?.present ? (
              <CdDeck
                status={cd}
                onPlayPause={() => onCdPlayPause?.()}
                onPrev={() => onCdPrev?.()}
                onNext={() => onCdNext?.()}
              />
            ) : null}
          </View>

          {wide ? null : <Groove />}

          <View style={{ flex: wide ? 0.92 : undefined, gap }}>
            <View style={{ gap: 10 }}>
              <View
                accessibilityElementsHidden
                importantForAccessibility="no-hide-descendants"
                style={{
                  flexDirection: "row",
                  alignItems: "flex-end",
                  justifyContent: "space-between",
                }}
              >
                <Text
                  maxFontSizeMultiplier={1.3}
                  style={[type.label, { color: colors.tertiaryLabel }]}
                >
                  Volume
                </Text>
                <LedReadout
                  value={String(Math.round(shownVolume))}
                  digits={3}
                  size={shortWide ? 36 : 44}
                  dim={volumeDisabled}
                  accessibilityLabel={`Volume ${Math.round(shownVolume)}`}
                  testID="volume-readout"
                />
              </View>
              <Fader
                orientation="horizontal"
                value={volume}
                min={0}
                max={100}
                step={1}
                origin={0}
                accessibilityStep={5}
                accessibilityLabel="Volume"
                formatValue={(value) => String(Math.round(value))}
                disabled={volumeDisabled}
                testID="volume-slider"
                onDraft={setShownVolume}
                onChange={onVolumeChange}
              />
              <Scale marks={["0", "50", "100"]} />
            </View>

            <Groove />

            <View
              testID="mixer-keys"
              onLayout={(event) => setKeysWidth(event.nativeEvent.layout.width)}
              style={{ flexDirection: "row", flexWrap: "wrap", gap: KEY_GAP }}
            >
              <PanelKey
                label="Source"
                accessibilityHint="Choose what the desktop captures"
                onPress={onChangeInput}
                style={keyStyle}
                testID="source-key"
              />
              <PanelKey
                label="Speaker"
                accessibilityHint="Choose where the audio plays"
                onPress={onChangeOutput}
                style={keyStyle}
                testID="speaker-key"
              />
              <PanelKey
                label="EQ"
                accessibilityLabel="Equalizer"
                accessibilityValue={{ text: eqFlat ? "Flat" : "Custom" }}
                indicator={!eqFlat}
                disabled={soundDisabled}
                onPress={onOpenEqualizer}
                style={keyStyle}
                testID="eq-key"
              />
              <PanelKey
                label="Format"
                accessibilityLabel="Audio format"
                disabled={soundDisabled}
                onPress={onOpenFormat}
                style={keyStyle}
                testID="format-key"
              />
            </View>
          </View>
        </View>
      </ScrollView>
    </Faceplate>
  );
}
