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
  type TextInputInstance,
  useWindowDimensions,
  View,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { useKeyboardClearance } from "./hooks/useKeyboardClearance";
import { colors, fonts, layout, radius, type } from "./theme";
import { Faceplate, Groove } from "./ui/faceplate";
import { icons } from "./ui/icons";
import { wellStyle } from "./ui/led-window";
import { NativeIcon } from "./ui/native-icon";
import { PanelKey } from "./ui/panel-key";
import { Sign } from "./ui/sign";

const DEFAULT_NAME = "on-air on Mac";

function DiscoveredRow({
  hit,
  selected,
  onPress,
}: {
  hit: DiscoveredHost;
  selected: boolean;
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
        minHeight: 78,
        paddingHorizontal: 12,
        paddingVertical: 12,
        flexDirection: "row",
        alignItems: "center",
        gap: 14,
        borderRadius: radius.sm,
        backgroundColor: selected || pressed ? colors.faceRow : "transparent",
      })}
    >
      <NativeIcon
        name={icons.computer}
        size={28}
        color={selected ? colors.amber : colors.secondaryLabel}
      />
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
  const pinInputRef = useRef<TextInputInstance>(null);
  // Lifts the whole code panel, Connect included, above the number pad (and its iOS "Done" bar).
  const keyboardClearance = useKeyboardClearance(pinInputRef);
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
      style={{ flex: 1, backgroundColor: colors.faceBottom }}
      testID="pairing-screen"
    >
      <Faceplate>
        <ScrollView
          ref={keyboardClearance.scrollRef}
          onScroll={keyboardClearance.onScroll}
          onLayout={keyboardClearance.onLayout}
          style={{ flex: 1 }}
          contentInsetAdjustmentBehavior="automatic"
          automaticallyAdjustKeyboardInsets
          keyboardShouldPersistTaps="handled"
          showsVerticalScrollIndicator={false}
          testID="pairing-scroll"
          contentContainerStyle={{
            minHeight: "100%",
            paddingTop: insets.top + (wide ? 24 : 16),
            // Clear of a side rail or sensor housing (iPhone Duo, landscape).
            paddingLeft: Math.max(insets.left, compact ? 16 : 20),
            paddingRight: Math.max(insets.right, compact ? 16 : 20),
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
                  style={[
                    type.body,
                    { color: colors.secondaryLabel, fontSize: 16, lineHeight: 22 },
                  ]}
                >
                  Keep on-air open on your Mac and use the code it shows. This is a one-time setup
                  on this device.
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
              <View
                style={{ width: wide ? undefined : "100%", flex: wide ? 1 : undefined, gap: 10 }}
              >
                <Text style={[type.label, { color: colors.tertiaryLabel }]}>Your Mac</Text>
                <View>
                  {found.length > 0 ? (
                    found.map((hit, index) => (
                      <View key={`${hit.host}:${hit.port}`}>
                        {index > 0 ? <Groove /> : null}
                        <DiscoveredRow
                          hit={hit}
                          selected={!manual && selected?.host === hit.host}
                          onPress={() => {
                            setManual(false);
                            onSelectHost(hit);
                          }}
                        />
                      </View>
                    ))
                  ) : (
                    <EmptyDiscovery scanning={scanning} />
                  )}
                </View>

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
                  <View testID="manual-setup" style={{ gap: 8 }}>
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
                      style={[
                        wellStyle,
                        {
                          minHeight: 50,
                          paddingHorizontal: 14,
                          fontFamily: fonts.mono,
                          color: colors.label,
                          fontSize: 17,
                        },
                      ]}
                    />
                  </View>
                ) : null}
              </View>

              {targetReady ? (
                <View
                  ref={keyboardClearance.targetRef}
                  onLayout={keyboardClearance.onLayout}
                  testID="pairing-code"
                  style={{ width: wide ? undefined : "100%", flex: wide ? 1 : undefined, gap: 10 }}
                >
                  <Text style={[type.label, { color: colors.tertiaryLabel }]}>Pairing code</Text>
                  <View style={{ gap: 18 }}>
                    <View style={{ flexDirection: "row", alignItems: "center", gap: 14 }}>
                      <NativeIcon name={icons.lock} size={28} color={colors.secondaryLabel} />
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
                      onFocus={keyboardClearance.onFocus}
                      editable={!pairing}
                      accessibilityLabel="Six-digit pairing code"
                      testID="pin-input"
                      style={[
                        wellStyle,
                        {
                          minHeight: 66,
                          paddingHorizontal: 14,
                          fontFamily: fonts.led,
                          color: colors.amber,
                          fontSize: compact ? 24 : 27,
                          letterSpacing: compact ? 6 : 9,
                          textAlign: "center",
                        },
                      ]}
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

                    <PanelKey
                      label="Connect"
                      lit={lit}
                      busy={pairing}
                      disabled={!canPair}
                      onPress={onPair}
                      testID="pair-button"
                    />
                  </View>
                </View>
              ) : null}
            </View>
          </View>
        </ScrollView>
      </Faceplate>
    </KeyboardAvoidingView>
  );
}
