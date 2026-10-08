import { Host, type IconName } from "@expo/ui";
import type { CdStatus } from "@on-air/api-types";
import type React from "react";
import { Pressable, ScrollView, Text, useWindowDimensions, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { NativeFader } from "./native-fader";
import { colors, layout, radius } from "./theme";
import { Card } from "./ui/card";
import { IconTile } from "./ui/icon-tile";
import { icons } from "./ui/icons";
import { NativeIcon } from "./ui/native-icon";
import { Notice } from "./ui/notice";

const ROW_PADDING = 18;
const TILE = 58;
const ARROW = 24;
const ROW_GAP = 16;

function RouteRow({
  icon,
  title,
  subtitle,
  accessibilityLabel,
  accessibilityHint,
  onPress,
  testID,
  dense = false,
}: {
  icon: IconName;
  title: string;
  subtitle: string;
  accessibilityLabel: string;
  accessibilityHint: string;
  onPress: () => void;
  testID: string;
  dense?: boolean;
}) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={accessibilityLabel}
      accessibilityHint={accessibilityHint}
      onPress={onPress}
      testID={testID}
      style={({ pressed }) => ({
        minHeight: dense ? 82 : 98,
        paddingHorizontal: ROW_PADDING,
        flexDirection: "row",
        alignItems: "center",
        gap: ROW_GAP,
        backgroundColor: pressed ? colors.surfacePressed : "transparent",
      })}
    >
      <IconTile name={icon} size="lg" />
      <View style={{ flex: 1, gap: 3 }}>
        <Text numberOfLines={1} style={{ color: colors.label, fontSize: 20, fontWeight: "700" }}>
          {title}
        </Text>
        <Text numberOfLines={1} style={{ color: colors.secondaryLabel, fontSize: 16 }}>
          {subtitle}
        </Text>
      </View>
      <NativeIcon name={icons.chevron} size={19} color={colors.secondaryLabel} />
    </Pressable>
  );
}

/** The separator between the two route rows, with the arrow centred under the source tile. */
function RouteDivider() {
  const arrowInset = ROW_PADDING + TILE / 2 - ARROW / 2;
  return (
    <View
      pointerEvents="none"
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      style={{
        height: ARROW,
        marginVertical: -ARROW / 2,
        zIndex: 1,
        flexDirection: "row",
        alignItems: "center",
      }}
    >
      <View
        style={{
          marginLeft: arrowInset,
          width: ARROW,
          height: ARROW,
          borderRadius: ARROW / 2,
          alignItems: "center",
          justifyContent: "center",
          backgroundColor: colors.surface,
        }}
      >
        <NativeIcon name={icons.route} size={16} color={colors.accent} />
      </View>
      <View
        style={{
          flex: 1,
          height: 1,
          marginLeft: ROW_PADDING + TILE + ROW_GAP - arrowInset - ARROW,
          backgroundColor: colors.separator,
        }}
      />
    </View>
  );
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
        <Text
          accessibilityRole="header"
          style={{ flex: 1, color: colors.label, fontSize: 38, fontWeight: "700" }}
        >
          on-air
        </Text>
        <View
          accessible
          accessibilityRole="text"
          accessibilityLabel={status.announcement}
          testID="status-pill"
          style={{ flexDirection: "row", alignItems: "center", gap: 7, paddingHorizontal: 14 }}
        >
          <View
            style={{
              width: 9,
              height: 9,
              borderRadius: 5,
              backgroundColor: status.live ? colors.accent : colors.tertiaryLabel,
            }}
          />
          <Text
            style={{
              color: status.live ? colors.accent : colors.secondaryLabel,
              fontSize: 17,
              fontWeight: "600",
            }}
          >
            {status.text}
          </Text>
        </View>
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="More options"
          onPress={onMore}
          testID="more-button"
          hitSlop={4}
          style={({ pressed }) => ({
            width: 46,
            height: 46,
            borderRadius: 23,
            alignItems: "center",
            justifyContent: "center",
            backgroundColor: pressed ? colors.surfacePressed : colors.surface,
            borderWidth: 1,
            borderColor: colors.border,
          })}
        >
          <NativeIcon name={icons.more} size={23} />
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
      style={{
        width: 46,
        height: 46,
        alignItems: "center",
        justifyContent: "center",
      }}
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
        <Text testID="cd-track" style={{ color: colors.accent, fontSize: 16, fontWeight: "700" }}>
          {padTrack(status.track)}/{padTrack(status.track_count)}
        </Text>
        <Text
          numberOfLines={1}
          style={{ flex: 1, color: colors.label, fontSize: 16, fontWeight: "600" }}
        >
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
  volumeDisabled,
  soundDisabled,
  cd,
  onChangeInput,
  onChangeOutput,
  onOpenSound,
  onOpenMore,
  onDisconnect,
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
  volumeDisabled: boolean;
  soundDisabled: boolean;
  cd?: CdStatus | null;
  onChangeInput: () => void;
  onChangeOutput: () => void;
  onOpenSound: () => void;
  onOpenMore: () => void;
  onDisconnect: () => void;
  onVolumeChange: (value: number) => void;
  onCdPlayPause?: () => void;
  onCdPrev?: () => void;
  onCdNext?: () => void;
}): React.JSX.Element {
  const insets = useSafeAreaInsets();
  const { width, height } = useWindowDimensions();
  const compact = width < layout.compactBreakpoint;
  const wide = width >= layout.wideBreakpoint;
  const shortWide = wide && height < 600;
  const source = activeInput || "Choose a source";
  const output = activeOutput || "Choose a speaker";
  const outputSubtitle = !activeOutput
    ? "No speaker selected"
    : outputPhase === "starting"
      ? "Connecting…"
      : outputPhase === "failed"
        ? "Speaker failed. Choose it again."
        : "Selected speaker";

  return (
    <View style={{ flex: 1, backgroundColor: colors.background }} testID="mixer-screen">
      <Header status={status} onMore={onOpenMore} />
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
            <View style={{ gap: 10 }}>
              <Text style={{ color: colors.secondaryLabel, fontSize: 17 }}>Now streaming</Text>
              <Card>
                <RouteRow
                  icon={icons.monitor}
                  title={source}
                  subtitle="This Mac"
                  accessibilityLabel={`Source: ${source}`}
                  accessibilityHint="Choose what the desktop captures"
                  onPress={onChangeInput}
                  testID="source-row"
                  dense={shortWide}
                />
                <RouteDivider />
                <RouteRow
                  icon={icons.speaker}
                  title={output}
                  subtitle={outputSubtitle}
                  accessibilityLabel={`Speaker: ${output}`}
                  accessibilityHint="Choose where the audio plays"
                  onPress={onChangeOutput}
                  testID="output-row"
                  dense={shortWide}
                />
              </Card>
            </View>

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
              <NativeIcon
                name={icons.live}
                size={shortWide ? 52 : compact ? 58 : 68}
                color={status.live ? colors.accent : colors.tertiaryLabel}
              />
              <View style={{ flexShrink: 1, gap: 5 }}>
                <Text style={{ color: colors.label, fontSize: 21, fontWeight: "700" }}>
                  {status.live ? "Audio is live" : "Ready to stream"}
                </Text>
                <Text
                  numberOfLines={2}
                  style={{ color: colors.secondaryLabel, fontSize: 16, lineHeight: 21 }}
                >
                  {`${source} → ${output}`}
                </Text>
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
            {wide ? (
              <Text style={{ color: colors.secondaryLabel, fontSize: 17 }}>Playback</Text>
            ) : null}
            <Card
              style={{ paddingHorizontal: 18, paddingVertical: 16, gap: 8, overflow: "visible" }}
            >
              <View
                accessibilityElementsHidden
                importantForAccessibility="no-hide-descendants"
                style={{ paddingLeft: 40, flexDirection: "row", alignItems: "center" }}
              >
                <Text style={{ flex: 1, color: colors.label, fontSize: 16, fontWeight: "600" }}>
                  Volume
                </Text>
                <Text style={{ color: colors.label, fontSize: 16, fontWeight: "600" }}>
                  {Math.round(volume)}
                </Text>
              </View>
              <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
                <NativeIcon
                  name={icons.volume}
                  size={28}
                  color={volumeDisabled ? colors.disabled : colors.label}
                />
                <Host
                  colorScheme="dark"
                  seedColor={colors.accent}
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

            <Pressable
              accessibilityRole="button"
              accessibilityLabel="Sound settings"
              accessibilityState={{ disabled: soundDisabled }}
              onPress={onOpenSound}
              disabled={soundDisabled}
              testID="sound-settings-button"
              style={({ pressed }) => ({
                minHeight: 68,
                paddingHorizontal: 18,
                flexDirection: "row",
                alignItems: "center",
                gap: 16,
                borderRadius: radius.xl,
                borderCurve: "continuous",
                borderWidth: 1,
                borderColor: colors.border,
                backgroundColor: pressed ? colors.surfacePressed : colors.surface,
                opacity: soundDisabled ? 0.5 : 1,
              })}
            >
              <NativeIcon name={icons.tune} size={28} />
              <Text style={{ flex: 1, color: colors.label, fontSize: 18 }}>Sound settings</Text>
              <NativeIcon name={icons.chevron} size={19} color={colors.secondaryLabel} />
            </Pressable>

            <Pressable
              accessibilityRole="button"
              accessibilityLabel="Disconnect remote"
              onPress={onDisconnect}
              testID="disconnect-button"
              style={({ pressed }) => ({
                minHeight: 48,
                alignItems: "center",
                justifyContent: "center",
                opacity: pressed ? 0.65 : 1,
              })}
            >
              <Text style={{ color: colors.accent, fontSize: 17, fontWeight: "500" }}>
                Disconnect remote
              </Text>
            </Pressable>
          </View>
        </View>
      </ScrollView>
    </View>
  );
}
