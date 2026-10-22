import { Host, type IconName } from "@expo/ui";
import type { CdStatus, FirewallWarning } from "@on-air/api-types";
import type React from "react";
import { Pressable, ScrollView, Text, useWindowDimensions, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { NativeFader } from "./native-fader";
import { colors, layout, radius, type } from "./theme";
import { Card } from "./ui/card";
import { FirewallNotice } from "./ui/firewall-notice";
import { icons } from "./ui/icons";
import { LedReadout } from "./ui/led-readout";
import { NativeIcon } from "./ui/native-icon";
import { Notice } from "./ui/notice";
import { PanelKey } from "./ui/panel-key";
import { Sign } from "./ui/sign";

/** Section caption ("Now streaming", "Playback"), like the desktop's SOURCE / DESTINATION. */
function SectionLabel({ children }: { children: string }) {
  return <Text style={[type.label, { color: colors.tertiaryLabel }]}>{children}</Text>;
}

type HeaderStatus = { text: string; announcement: string; live: boolean };

function Header({ status, onMore }: { status: HeaderStatus; onMore: () => void }) {
  const insets = useSafeAreaInsets();
  return (
    <View
      style={{
        paddingTop: insets.top + 10,
        paddingBottom: 14,
        borderBottomWidth: 1,
        borderBottomColor: colors.separator,
      }}
    >
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
          <Sign lit={status.live} testID="mixer-sign" />
        </View>
        <View
          accessible
          accessibilityRole="text"
          accessibilityLabel={status.announcement}
          testID="status-pill"
          style={{ flexDirection: "row", alignItems: "center", gap: 7, paddingHorizontal: 14 }}
        >
          <View
            style={{
              width: 8,
              height: 8,
              borderRadius: 4,
              backgroundColor: status.live ? colors.accent : colors.liveDim,
            }}
          />
          <Text style={[type.label, { color: status.live ? colors.accent : colors.tertiaryLabel }]}>
            {status.text}
          </Text>
        </View>
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="More options"
          onPress={onMore}
          testID="more-button"
          hitSlop={5}
          style={({ pressed }) => ({
            width: 36,
            height: 36,
            borderRadius: 18,
            alignItems: "center",
            justifyContent: "center",
            backgroundColor: pressed ? colors.surfacePressed : colors.surface,
            borderWidth: 1,
            borderColor: colors.border,
          })}
        >
          <NativeIcon name={icons.more} size={19} />
        </Pressable>
      </View>
    </View>
  );
}

function padTrack(n: number): string {
  return String(n).padStart(2, "0");
}

function TransportButton({
  icon,
  size,
  label,
  onPress,
  testID,
}: {
  icon: IconName;
  size: number;
  label: string;
  onPress: () => void;
  testID: string;
}) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      onPress={onPress}
      testID={testID}
      hitSlop={8}
      style={({ pressed }) => ({
        width: 46,
        height: 46,
        alignItems: "center",
        justifyContent: "center",
        borderRadius: radius.md,
        borderWidth: 1,
        borderColor: colors.border,
        borderTopColor: colors.borderStrong,
        backgroundColor: pressed ? colors.surfacePressed : colors.surfaceRaised,
      })}
    >
      <NativeIcon name={icon} size={size} />
    </Pressable>
  );
}

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
    <Card testID="cd-transport" style={{ paddingHorizontal: 18, paddingVertical: 14, gap: 12 }}>
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
      <View style={{ flexDirection: "row", justifyContent: "center", gap: 22 }}>
        <TransportButton
          icon={icons.prev}
          size={28}
          label="Previous track"
          onPress={onPrev}
          testID="cd-prev"
        />
        <TransportButton
          icon={status.playing ? icons.pause : icons.play}
          size={32}
          label={status.playing ? "Pause" : "Play"}
          onPress={onPlayPause}
          testID="cd-play"
        />
        <TransportButton
          icon={icons.next}
          size={28}
          label="Next track"
          onPress={onNext}
          testID="cd-next"
        />
      </View>
    </Card>
  );
}

export function MixerHome({
  activeInput,
  activeOutput,
  outputPhase,
  volume,
  status,
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
  status: HeaderStatus;
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
  // The four keys share a row until large text or a narrow screen would crowd them.
  const keyGrid = fontScale > 1.3 || width < 340;
  const keyStyle = { flexGrow: 1, flexBasis: keyGrid ? "48%" : 0 } as const;
  const source = activeInput || "Choose a source";
  const output = activeOutput || "Choose a speaker";
  const speakerNote =
    activeOutput && outputPhase === "starting"
      ? "Connecting…"
      : activeOutput && outputPhase === "failed"
        ? "Speaker failed. Choose it again."
        : null;
  // What the header shows; a firewall block overrides live, and the lamp
  // follows the header so the two never disagree.
  const headerStatus: HeaderStatus = firewallWarning
    ? { text: "Audio blocked", announcement: "Firewall blocking audio", live: false }
    : status;

  return (
    <View style={{ flex: 1, backgroundColor: colors.background }} testID="mixer-screen">
      <Header status={headerStatus} onMore={onOpenMore} />
      <ScrollView
        style={{ flex: 1 }}
        contentInsetAdjustmentBehavior="automatic"
        showsVerticalScrollIndicator={false}
        contentContainerStyle={{
          paddingHorizontal: compact ? 16 : 20,
          paddingTop: shortWide ? 12 : 22,
          paddingBottom: Math.max(insets.bottom, 18) + 12,
          gap: compact ? 18 : 22,
          width: "100%",
          maxWidth: layout.maxWidth,
          alignSelf: "center",
        }}
      >
        {error ? <Notice message={error} testID="mixer-error" /> : null}
        <FirewallNotice warning={firewallWarning} />
        <View
          testID={
            shortWide ? "mixer-landscape-layout" : wide ? "mixer-wide-layout" : "mixer-phone-layout"
          }
          style={{
            width: "100%",
            flexDirection: wide ? "row" : "column",
            alignItems: "flex-start",
            gap: shortWide ? 16 : wide ? 24 : compact ? 18 : 22,
          }}
        >
          <View
            style={{
              width: wide ? undefined : "100%",
              flex: wide ? 1.08 : undefined,
              gap: shortWide ? 12 : wide ? 24 : compact ? 18 : 22,
            }}
          >
            {cd?.present ? (
              <CdDeck
                status={cd}
                onPlayPause={() => onCdPlayPause?.()}
                onPrev={() => onCdPrev?.()}
                onNext={() => onCdNext?.()}
              />
            ) : null}

            <View
              accessible
              accessibilityLabel={`${status.live ? "Audio is live" : "Ready to stream"}. ${source} to ${output}`}
              style={{
                minHeight: shortWide ? 72 : wide ? 112 : 86,
                paddingHorizontal: compact ? 8 : 18,
                flexDirection: "row",
                alignItems: "center",
                justifyContent: "center",
                gap: shortWide ? 12 : 18,
              }}
            >
              <View
                testID="live-lamp"
                style={{
                  borderRadius: 40,
                  shadowColor: colors.accent,
                  shadowOffset: { width: 0, height: 0 },
                  shadowRadius: 12,
                  shadowOpacity: headerStatus.live ? 0.6 : 0,
                }}
              >
                <NativeIcon
                  name={icons.live}
                  size={shortWide ? 52 : compact ? 58 : 68}
                  color={headerStatus.live ? colors.accent : colors.liveDim}
                />
              </View>
              <View style={{ flexShrink: 1, gap: 5 }}>
                <Text style={[type.title, { color: colors.label, fontSize: 20, lineHeight: 25 }]}>
                  {status.live ? "Audio is live" : "Ready to stream"}
                </Text>
                <Text numberOfLines={2} style={[type.body, { color: colors.secondaryLabel }]}>
                  {`${source} → ${output}`}
                </Text>
                {speakerNote ? (
                  <Text style={[type.body, { color: colors.secondaryLabel }]}>{speakerNote}</Text>
                ) : null}
              </View>
            </View>
          </View>

          <View
            style={{
              width: wide ? undefined : "100%",
              flex: wide ? 0.92 : undefined,
              gap: shortWide ? 12 : wide ? 18 : compact ? 18 : 22,
            }}
          >
            {wide ? <SectionLabel>Playback</SectionLabel> : null}
            <Card
              style={{ paddingHorizontal: 18, paddingVertical: 16, gap: 8, overflow: "visible" }}
            >
              <View
                accessibilityElementsHidden
                importantForAccessibility="no-hide-descendants"
                style={{ paddingLeft: 40, flexDirection: "row", alignItems: "center" }}
              >
                <Text style={[type.label, { flex: 1, color: colors.secondaryLabel }]}>Volume</Text>
                <LedReadout
                  value={String(Math.round(volume))}
                  digits={3}
                  size={20}
                  dim={volumeDisabled}
                  accessibilityLabel={`Volume ${Math.round(volume)}`}
                  testID="volume-readout"
                />
              </View>
              <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
                <NativeIcon
                  name={icons.volume}
                  size={28}
                  color={volumeDisabled ? colors.disabled : colors.label}
                />
                <Host
                  colorScheme="dark"
                  seedColor={colors.amber}
                  ignoreSafeArea="all"
                  style={{ flex: 1, height: 42 }}
                >
                  <NativeFader
                    label="Volume"
                    accessibilityLabel="Volume"
                    value={volume}
                    min={0}
                    max={100}
                    testId="volume-slider"
                    disabled={volumeDisabled}
                    hideHeader
                    onChange={onVolumeChange}
                  />
                </Host>
              </View>
            </Card>

            <View testID="mixer-keys" style={{ flexDirection: "row", flexWrap: "wrap", gap: 8 }}>
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
    </View>
  );
}
