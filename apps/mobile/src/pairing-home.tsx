import type { DiscoveredHost } from "@on-air/api-types";
import type React from "react";
import { useRef, useState } from "react";
import {
  ActivityIndicator,
  KeyboardAvoidingView,
  Platform,
  Pressable,
  ScrollView,
  Text,
  TextInput,
  useWindowDimensions,
  View,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { colors, fonts, layout, radius, type } from "./theme";
import { Card } from "./ui/card";
import { IconTile } from "./ui/icon-tile";
import { icons } from "./ui/icons";
import { NativeIcon } from "./ui/native-icon";
import { Sign } from "./ui/sign";

const DEFAULT_NAME = "on-air on Mac";

function DiscoveredRow({
  hit,
  selected,
  last,
  onPress,
}: {
  hit: DiscoveredHost;
  selected: boolean;
  last: boolean;
  onPress: () => void;
}) {
  const name = hit.name ?? DEFAULT_NAME;
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${name}, ${hit.host}`}
      accessibilityHint={selected ? undefined : "Selects this Mac for pairing"}
      accessibilityState={{ selected }}
      onPress={onPress}
      testID={`discovered-${hit.host}`}
      style={({ pressed }) => ({
        minHeight: 86,
        paddingHorizontal: 16,
        paddingVertical: 14,
        flexDirection: "row",
        alignItems: "center",
        gap: 14,
        borderBottomWidth: last ? 0 : 1,
        borderBottomColor: colors.separator,
        borderWidth: selected ? 1 : 0,
        borderColor: colors.key,
        backgroundColor: selected
          ? colors.surfaceRaised
          : pressed
            ? colors.surfacePressed
            : "transparent",
      })}
    >
      <IconTile name={icons.computer} />
      <View style={{ flex: 1, gap: 3 }}>
        <Text numberOfLines={1} style={[type.title, { color: colors.label }]}>
          {name}
        </Text>
        <Text numberOfLines={1} style={[type.body, { color: colors.secondaryLabel }]}>
          {selected ? "Ready for your code" : "Nearby"}
        </Text>
      </View>
      <NativeIcon
        name={icons.check}
        size={24}
        color={selected ? colors.amber : colors.tertiaryLabel}
      />
    </Pressable>
  );
}

function EmptyDiscovery({ scanning }: { scanning: boolean }) {
  return (
    <View
      accessible
      accessibilityLiveRegion="polite"
      style={{
        minHeight: 148,
        paddingHorizontal: 22,
        paddingVertical: 22,
        alignItems: "center",
        justifyContent: "center",
        gap: 12,
      }}
    >
      {scanning ? (
        <ActivityIndicator size="large" color={colors.amber} />
      ) : (
        <NativeIcon name={icons.search} size={34} color={colors.secondaryLabel} />
      )}
      <View style={{ alignItems: "center", gap: 4 }}>
        <Text style={[type.title, { color: colors.label }]}>
          {scanning ? "Looking for your Mac…" : "No Mac found yet"}
        </Text>
        <Text style={[type.body, { color: colors.secondaryLabel, textAlign: "center" }]}>
          {scanning
            ? "Make sure both devices are on the same Wi-Fi."
            : "Open on-air on your Mac, then search again."}
        </Text>
      </View>
    </View>
  );
}

function LinkButton({
  label,
  accessibilityLabel,
  onPress,
  disabled = false,
  tone,
  testID,
}: {
  label: string;
  accessibilityLabel?: string;
  onPress: () => void;
  disabled?: boolean;
  tone: "accent" | "secondary";
  testID: string;
}) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={accessibilityLabel ?? label}
      accessibilityState={{ disabled }}
      disabled={disabled}
      onPress={onPress}
      testID={testID}
      style={({ pressed }) => ({
        minHeight: layout.minTarget,
        paddingHorizontal: 12,
        alignItems: "center",
        justifyContent: "center",
        opacity: disabled ? 0.55 : pressed ? 0.65 : 1,
      })}
    >
      <Text
        style={[
          type.body,
          { fontSize: 16, color: tone === "accent" ? colors.amber : colors.secondaryLabel },
        ]}
      >
        {label}
      </Text>
    </Pressable>
  );
}

export function PairingHome({
  scanning,
  pairing,
  found,
  host,
  pin,
  error,
  onScan,
  onSelectHost,
  onChangeHost,
  onChangePin,
  onPair,
}: {
  scanning: boolean;
  pairing: boolean;
  found: DiscoveredHost[];
  host: string;
  pin: string;
  error?: string | null;
  onScan: () => void;
  onSelectHost: (hit: DiscoveredHost) => void;
  onChangeHost: (host: string) => void;
  onChangePin: (pin: string) => void;
  onPair: () => void;
}): React.JSX.Element {
  const insets = useSafeAreaInsets();
  const { width } = useWindowDimensions();
  const compact = width < layout.compactBreakpoint;
  const wide = width >= layout.wideBreakpoint;
  const [manual, setManual] = useState(false);
  const pinInputRef = useRef<TextInput>(null);
  const selected = found.find((hit) => hit.host === host.trim()) ?? null;
  const targetReady = manual ? host.trim().length > 0 : selected !== null;
  // The key stays lit while the code is checked so its spinner stays visible;
  // it is only pressable when lit and idle.
  const lit = targetReady && pin.length === 6;
  const canPair = lit && !pairing;

  return (
    // iOS insets the ScrollView and scrolls the focused field above the keyboard. Edge-to-edge
    // Android no longer resizes the window for it (adjustResize is a no-op), so pad instead.
    <KeyboardAvoidingView
      behavior={Platform.OS === "android" ? "padding" : undefined}
      style={{ flex: 1, backgroundColor: colors.background }}
      testID="pairing-screen"
    >
      <ScrollView
        style={{ flex: 1 }}
        contentInsetAdjustmentBehavior="automatic"
        automaticallyAdjustKeyboardInsets
        keyboardShouldPersistTaps="handled"
        showsVerticalScrollIndicator={false}
        testID="pairing-scroll"
        contentContainerStyle={{
          minHeight: "100%",
          paddingTop: insets.top + (wide ? 24 : 16),
          paddingHorizontal: compact ? 16 : 20,
          paddingBottom: Math.max(insets.bottom, 20) + 24,
          width: "100%",
          maxWidth: layout.maxWidth,
          alignSelf: "center",
        }}
      >
        <View style={{ gap: wide ? 28 : 24 }}>
          <View style={{ gap: 18 }}>
            <Sign lit={false} testID="remote-title" />
            <View style={{ maxWidth: 620, gap: 8 }}>
              <Text testID="core-status" style={[type.label, { color: colors.amber }]}>
                Pair your Mac
              </Text>
              <Text
                accessibilityRole="header"
                style={[
                  type.title,
                  { color: colors.label, fontSize: wide ? 30 : 26, lineHeight: wide ? 36 : 32 },
                ]}
              >
                Connect this device to on-air
              </Text>
              <Text
                style={[type.body, { color: colors.secondaryLabel, fontSize: 16, lineHeight: 22 }]}
              >
                Keep on-air open on your Mac and use the code it shows. This is a one-time setup on
                this device.
              </Text>
            </View>
          </View>

          <View
            testID={wide ? "pairing-wide-layout" : "pairing-phone-layout"}
            style={{
              width: "100%",
              flexDirection: wide ? "row" : "column",
              alignItems: "flex-start",
              gap: wide ? 22 : 20,
            }}
          >
            <View style={{ width: wide ? undefined : "100%", flex: wide ? 1 : undefined, gap: 10 }}>
              <Text style={[type.label, { color: colors.tertiaryLabel }]}>Your Mac</Text>
              <Card>
                {found.length > 0 ? (
                  found.map((hit, index) => (
                    <DiscoveredRow
                      key={`${hit.host}:${hit.port}`}
                      hit={hit}
                      selected={!manual && selected?.host === hit.host}
                      last={index === found.length - 1}
                      onPress={() => {
                        setManual(false);
                        onSelectHost(hit);
                      }}
                    />
                  ))
                ) : (
                  <EmptyDiscovery scanning={scanning} />
                )}
              </Card>

              <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
                <LinkButton
                  label={scanning ? "Searching…" : "Search again"}
                  accessibilityLabel={scanning ? "Searching for Macs" : "Search for Macs again"}
                  disabled={scanning}
                  onPress={onScan}
                  tone="accent"
                  testID="scan-button"
                />
                <View style={{ width: 1, height: 18, backgroundColor: colors.separator }} />
                <LinkButton
                  label={manual ? "Hide manual setup" : "Set up manually"}
                  onPress={() => setManual((current) => !current)}
                  tone="secondary"
                  testID="manual-setup-button"
                />
              </View>

              {manual ? (
                <Card testID="manual-setup" style={{ padding: 16, gap: 8 }}>
                  <Text style={[type.label, { color: colors.tertiaryLabel }]}>
                    Mac network address
                  </Text>
                  <TextInput
                    value={host}
                    onChangeText={onChangeHost}
                    placeholder="192.168.1.20"
                    placeholderTextColor={colors.tertiaryLabel}
                    cursorColor={colors.amber}
                    selectionHandleColor={colors.amber}
                    autoCapitalize="none"
                    autoCorrect={false}
                    keyboardType="url"
                    returnKeyType="next"
                    submitBehavior="submit"
                    onSubmitEditing={() => pinInputRef.current?.focus()}
                    accessibilityLabel="Mac network address"
                    testID="host-input"
                    style={{
                      minHeight: 50,
                      paddingHorizontal: 14,
                      ...type.body,
                      color: colors.label,
                      fontSize: 17,
                      borderRadius: radius.md,
                      borderCurve: "continuous",
                      backgroundColor: colors.iconSurface,
                      borderWidth: 1,
                      borderColor: colors.border,
                    }}
                  />
                </Card>
              ) : null}
            </View>

            {targetReady ? (
              <View
                style={{ width: wide ? undefined : "100%", flex: wide ? 1 : undefined, gap: 10 }}
              >
                <Text style={[type.label, { color: colors.tertiaryLabel }]}>Pairing code</Text>
                <Card style={{ padding: 18, gap: 18 }}>
                  <View style={{ flexDirection: "row", alignItems: "center", gap: 14 }}>
                    <IconTile name={icons.lock} />
                    <View style={{ flex: 1, gap: 3 }}>
                      <Text style={[type.title, { color: colors.label }]}>
                        Enter the code on your Mac
                      </Text>
                      <Text style={[type.body, { color: colors.secondaryLabel }]}>
                        We'll remember this Mac after you connect.
                      </Text>
                    </View>
                  </View>

                  <TextInput
                    ref={pinInputRef}
                    value={pin}
                    onChangeText={onChangePin}
                    placeholder="888888"
                    placeholderTextColor={colors.ledGhost}
                    cursorColor={colors.amber}
                    selectionHandleColor={colors.amber}
                    maxFontSizeMultiplier={1.3}
                    keyboardType="number-pad"
                    inputMode="numeric"
                    maxLength={6}
                    returnKeyType="done"
                    onSubmitEditing={onPair}
                    editable={!pairing}
                    accessibilityLabel="Six-digit pairing code"
                    testID="pin-input"
                    style={{
                      minHeight: 66,
                      paddingHorizontal: 14,
                      fontFamily: fonts.led,
                      color: colors.amber,
                      fontSize: compact ? 24 : 27,
                      letterSpacing: compact ? 6 : 9,
                      textAlign: "center",
                      borderRadius: radius.md,
                      borderCurve: "continuous",
                      backgroundColor: colors.ledWell,
                      borderWidth: 1,
                      borderColor: pin.length === 6 ? colors.key : colors.separator,
                    }}
                  />

                  {error ? (
                    <Text
                      accessibilityRole="alert"
                      testID="pairing-error"
                      style={[type.body, { color: colors.onError, fontSize: 14, lineHeight: 19 }]}
                    >
                      {error}
                    </Text>
                  ) : null}

                  <Pressable
                    accessibilityRole="button"
                    accessibilityLabel="Connect"
                    accessibilityState={{ disabled: !canPair, busy: pairing }}
                    disabled={!canPair}
                    onPress={onPair}
                    testID="pair-button"
                    style={({ pressed }) => ({
                      minHeight: 54,
                      borderRadius: radius.md,
                      borderCurve: "continuous",
                      alignItems: "center",
                      justifyContent: "center",
                      borderWidth: 1,
                      borderColor: lit ? colors.amberBright : colors.border,
                      backgroundColor: lit ? colors.amber : colors.surface,
                      opacity: pressed ? 0.7 : 1,
                    })}
                  >
                    {pairing ? (
                      <ActivityIndicator color={colors.onAmber} />
                    ) : (
                      <Text
                        style={[
                          type.label,
                          {
                            fontSize: 13,
                            color: lit ? colors.onAmber : colors.tertiaryLabel,
                          },
                        ]}
                      >
                        Connect
                      </Text>
                    )}
                  </Pressable>
                </Card>
              </View>
            ) : null}
          </View>
        </View>
      </ScrollView>
    </KeyboardAvoidingView>
  );
}
