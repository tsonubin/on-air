import { Host, Icon, type IconName } from "@expo/ui";
import type React from "react";
import { Pressable, ScrollView, Text, useWindowDimensions, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { NativeFader } from "./native-fader";

const icons = {
  monitor: Icon.select({
    ios: "display",
    android: import("@expo/material-symbols/monitor.xml"),
  }),
  speaker: Icon.select({
    ios: "hifispeaker.fill",
    android: import("@expo/material-symbols/speaker.xml"),
  }),
  chevron: Icon.select({
    ios: "chevron.right",
    android: import("@expo/material-symbols/chevron_right.xml"),
  }),
  tune: Icon.select({
    ios: "slider.horizontal.3",
    android: import("@expo/material-symbols/tune.xml"),
  }),
  more: Icon.select({
    ios: "ellipsis",
    android: import("@expo/material-symbols/more_horiz.xml"),
  }),
  volume: Icon.select({
    ios: "speaker.wave.2.fill",
    android: import("@expo/material-symbols/volume_up.xml"),
  }),
  live: Icon.select({
    ios: "record.circle",
    android: import("@expo/material-symbols/radio_button_checked.xml"),
  }),
  route: Icon.select({
    ios: "arrow.down",
    android: import("@expo/material-symbols/south.xml"),
  }),
} satisfies Record<string, IconName>;

export const mobileColors = {
  background: "#000000",
  surface: "#171719",
  surfacePressed: "#202023",
  iconSurface: "#232326",
  border: "#343438",
  separator: "#2b2b2f",
  label: "#f5f5f7",
  secondaryLabel: "#a1a1a7",
  tertiaryLabel: "#727278",
  accent: "#ff453a",
  disabled: "#5b5b61",
} as const;

function NativeIcon({
  name,
  size = 24,
  color = mobileColors.label,
  accessibilityLabel,
}: {
  name: IconName;
  size?: number;
  color?: string;
  accessibilityLabel?: string;
}) {
  return (
    <Host
      matchContents
      ignoreSafeArea="all"
      colorScheme="dark"
      seedColor={mobileColors.accent}
      style={{ width: size, height: size }}
    >
      <Icon name={name} size={size} color={color} accessibilityLabel={accessibilityLabel} />
    </Host>
  );
}

function RoundIcon({ name, label }: { name: IconName; label: string }) {
  return (
    <View
      accessibilityElementsHidden
      style={{
        width: 58,
        height: 58,
        borderRadius: 17,
        borderCurve: "continuous",
        alignItems: "center",
        justifyContent: "center",
        backgroundColor: mobileColors.iconSurface,
        borderWidth: 1,
        borderColor: mobileColors.border,
      }}
    >
      <NativeIcon name={name} size={30} accessibilityLabel={label} />
    </View>
  );
}

function RouteRow({
  icon,
  title,
  subtitle,
  onPress,
  testID,
  dense = false,
}: {
  icon: IconName;
  title: string;
  subtitle: string;
  onPress: () => void;
  testID: string;
  dense?: boolean;
}) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`Change ${title}`}
      onPress={onPress}
      testID={testID}
      style={({ pressed }) => ({
        minHeight: dense ? 82 : 98,
        paddingHorizontal: 18,
        flexDirection: "row",
        alignItems: "center",
        gap: 16,
        backgroundColor: pressed ? mobileColors.surfacePressed : "transparent",
      })}
    >
      <RoundIcon name={icon} label={title} />
      <View style={{ flex: 1, gap: 3 }}>
        <Text
          selectable
          numberOfLines={1}
          style={{ color: mobileColors.label, fontSize: 20, fontWeight: "700" }}
        >
          {title}
        </Text>
        <Text
          selectable
          numberOfLines={1}
          style={{ color: mobileColors.secondaryLabel, fontSize: 16 }}
        >
          {subtitle}
        </Text>
      </View>
      <NativeIcon
        name={icons.chevron}
        size={19}
        color={mobileColors.secondaryLabel}
        accessibilityLabel="Change"
      />
    </Pressable>
  );
}

function Header({
  live,
  statusText,
  onMore,
}: {
  live: boolean;
  statusText: string;
  onMore: () => void;
}) {
  const insets = useSafeAreaInsets();
  return (
    <View
      style={{
        paddingTop: insets.top + 10,
        paddingBottom: 14,
        borderBottomWidth: 1,
        borderBottomColor: mobileColors.separator,
      }}
    >
      <View
        style={{
          width: "100%",
          maxWidth: 960,
          alignSelf: "center",
          paddingHorizontal: 20,
          flexDirection: "row",
          alignItems: "center",
        }}
      >
        <Text
          selectable
          accessibilityRole="header"
          style={{
            flex: 1,
            color: mobileColors.label,
            fontSize: 38,
            fontWeight: "700",
          }}
        >
          on-air
        </Text>
        <View
          accessibilityLabel={live ? "Audio is live" : "Audio is ready"}
          style={{
            flexDirection: "row",
            alignItems: "center",
            gap: 7,
            paddingHorizontal: 14,
          }}
        >
          <View
            style={{
              width: 9,
              height: 9,
              borderRadius: 5,
              backgroundColor: live ? mobileColors.accent : mobileColors.tertiaryLabel,
            }}
          />
          <Text
            selectable
            style={{
              color: live ? mobileColors.accent : mobileColors.secondaryLabel,
              fontSize: 17,
              fontWeight: "600",
            }}
          >
            {statusText}
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
            backgroundColor: pressed ? mobileColors.surfacePressed : mobileColors.surface,
            borderWidth: 1,
            borderColor: mobileColors.border,
          })}
        >
          <NativeIcon name={icons.more} size={23} accessibilityLabel="More options" />
        </Pressable>
      </View>
    </View>
  );
}

export function MixerHome({
  activeInput,
  activeOutput,
  volume,
  live,
  statusText,
  error,
  volumeDisabled,
  soundDisabled,
  onChangeInput,
  onChangeOutput,
  onOpenSound,
  onOpenMore,
  onDisconnect,
  onVolumeChange,
}: {
  activeInput: string;
  activeOutput: string;
  volume: number;
  live: boolean;
  statusText: string;
  error?: string | null;
  volumeDisabled: boolean;
  soundDisabled: boolean;
  onChangeInput: () => void;
  onChangeOutput: () => void;
  onOpenSound: () => void;
  onOpenMore: () => void;
  onDisconnect: () => void;
  onVolumeChange: (value: number) => void;
}): React.JSX.Element {
  const insets = useSafeAreaInsets();
  const { width, height } = useWindowDimensions();
  const compact = width < 370;
  const wide = width >= 700;
  const shortWide = wide && height < 600;
  const source = activeInput || "Choose a source";
  const output = activeOutput || "Choose a speaker";

  return (
    <View style={{ flex: 1, backgroundColor: mobileColors.background }} testID="mixer-screen">
      <Header live={live} statusText={statusText} onMore={onOpenMore} />
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
          maxWidth: 960,
          alignSelf: "center",
        }}
      >
        {error ? (
          <View
            style={{
              paddingHorizontal: 14,
              paddingVertical: 12,
              borderRadius: 14,
              borderCurve: "continuous",
              backgroundColor: "#2b1213",
              borderWidth: 1,
              borderColor: "#68201f",
            }}
          >
            <Text
              selectable
              accessibilityRole="alert"
              style={{ color: "#ffb4ab", fontSize: 15, lineHeight: 20 }}
            >
              {error}
            </Text>
          </View>
        ) : null}
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
              <Text selectable style={{ color: mobileColors.secondaryLabel, fontSize: 17 }}>
                Now streaming
              </Text>
              <View
                style={{
                  overflow: "hidden",
                  backgroundColor: mobileColors.surface,
                  borderRadius: 20,
                  borderCurve: "continuous",
                  borderWidth: 1,
                  borderColor: mobileColors.border,
                }}
              >
                <RouteRow
                  icon={icons.monitor}
                  title={source}
                  subtitle="This Mac"
                  onPress={onChangeInput}
                  testID="source-row"
                  dense={shortWide}
                />
                <View
                  style={{ height: 1, backgroundColor: mobileColors.separator, marginLeft: 92 }}
                />
                <View
                  pointerEvents="none"
                  style={{
                    position: "absolute",
                    left: 39,
                    top: shortWide ? 71 : 87,
                    width: 24,
                    height: 24,
                    alignItems: "center",
                    justifyContent: "center",
                    borderRadius: 12,
                    backgroundColor: mobileColors.surface,
                  }}
                >
                  <NativeIcon
                    name={icons.route}
                    size={16}
                    color={mobileColors.accent}
                    accessibilityLabel="Routes to"
                  />
                </View>
                <RouteRow
                  icon={icons.speaker}
                  title={output}
                  subtitle={activeOutput ? "Connected" : "Not connected"}
                  onPress={onChangeOutput}
                  testID="output-row"
                  dense={shortWide}
                />
              </View>
            </View>

            <View
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
                color={live ? mobileColors.accent : mobileColors.tertiaryLabel}
                accessibilityLabel={live ? "Live" : "Ready"}
              />
              <View style={{ flexShrink: 1, gap: 5 }}>
                <Text
                  selectable
                  style={{ color: mobileColors.label, fontSize: 21, fontWeight: "700" }}
                >
                  {live ? "Audio is live" : "Ready to stream"}
                </Text>
                <Text
                  selectable
                  numberOfLines={2}
                  style={{ color: mobileColors.secondaryLabel, fontSize: 16, lineHeight: 21 }}
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
              <Text selectable style={{ color: mobileColors.secondaryLabel, fontSize: 17 }}>
                Playback
              </Text>
            ) : null}
            <View
              style={{
                paddingHorizontal: 18,
                paddingVertical: 16,
                gap: 8,
                backgroundColor: mobileColors.surface,
                borderRadius: 20,
                borderCurve: "continuous",
                borderWidth: 1,
                borderColor: mobileColors.border,
              }}
            >
              <View style={{ paddingLeft: 40, flexDirection: "row", alignItems: "center" }}>
                <Text
                  selectable
                  style={{ flex: 1, color: mobileColors.label, fontSize: 16, fontWeight: "600" }}
                >
                  Volume
                </Text>
                <Text
                  selectable
                  style={{ color: mobileColors.label, fontSize: 16, fontWeight: "600" }}
                >
                  {Math.round(volume)}
                </Text>
              </View>
              <View style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
                <NativeIcon
                  name={icons.volume}
                  size={28}
                  color={volumeDisabled ? mobileColors.disabled : mobileColors.label}
                  accessibilityLabel="Volume"
                />
                <Host
                  colorScheme="dark"
                  seedColor={mobileColors.accent}
                  ignoreSafeArea="all"
                  style={{ flex: 1, height: 42 }}
                >
                  <NativeFader
                    label="Volume"
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
            </View>

            <Pressable
              accessibilityRole="button"
              accessibilityLabel="Open sound settings"
              onPress={onOpenSound}
              disabled={soundDisabled}
              testID="sound-settings-button"
              style={({ pressed }) => ({
                minHeight: 68,
                paddingHorizontal: 18,
                flexDirection: "row",
                alignItems: "center",
                gap: 16,
                borderRadius: 20,
                borderCurve: "continuous",
                borderWidth: 1,
                borderColor: mobileColors.border,
                backgroundColor: pressed ? mobileColors.surfacePressed : mobileColors.surface,
                opacity: soundDisabled ? 0.5 : 1,
              })}
            >
              <NativeIcon name={icons.tune} size={28} accessibilityLabel="Sound settings" />
              <Text selectable style={{ flex: 1, color: mobileColors.label, fontSize: 18 }}>
                Sound settings
              </Text>
              <NativeIcon
                name={icons.chevron}
                size={19}
                color={mobileColors.secondaryLabel}
                accessibilityLabel="Open"
              />
            </Pressable>

            <Pressable
              accessibilityRole="button"
              onPress={onDisconnect}
              testID="disconnect-button"
              style={({ pressed }) => ({
                minHeight: 48,
                alignItems: "center",
                justifyContent: "center",
                opacity: pressed ? 0.65 : 1,
              })}
            >
              <Text
                selectable
                style={{ color: mobileColors.accent, fontSize: 17, fontWeight: "500" }}
              >
                Disconnect remote
              </Text>
            </Pressable>
          </View>
        </View>
      </ScrollView>
    </View>
  );
}
