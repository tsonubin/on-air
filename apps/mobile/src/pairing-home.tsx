import { Host, Icon, type IconName } from "@expo/ui";
import type { DiscoveredHost } from "@on-air/api-types";
import type React from "react";
import { useRef, useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  Text,
  TextInput,
  useWindowDimensions,
  View,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { mobileColors } from "./mixer-home";

const icons = {
  computer: Icon.select({
    ios: "macbook",
    android: import("@expo/material-symbols/laptop_mac.xml"),
  }),
  search: Icon.select({
    ios: "magnifyingglass",
    android: import("@expo/material-symbols/search.xml"),
  }),
  lock: Icon.select({
    ios: "lock.fill",
    android: import("@expo/material-symbols/lock.xml"),
  }),
  check: Icon.select({
    ios: "checkmark.circle.fill",
    android: import("@expo/material-symbols/check_circle.xml"),
  }),
} satisfies Record<string, IconName>;

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

function IconTile({ name, label }: { name: IconName; label: string }) {
  return (
    <View
      accessibilityElementsHidden
      style={{
        width: 54,
        height: 54,
        borderRadius: 17,
        borderCurve: "continuous",
        alignItems: "center",
        justifyContent: "center",
        backgroundColor: mobileColors.iconSurface,
        borderWidth: 1,
        borderColor: mobileColors.border,
      }}
    >
      <NativeIcon name={name} size={28} accessibilityLabel={label} />
    </View>
  );
}

function SectionCard({ children }: { children: React.ReactNode }) {
  return (
    <View
      style={{
        overflow: "hidden",
        borderRadius: 22,
        borderCurve: "continuous",
        backgroundColor: mobileColors.surface,
        borderWidth: 1,
        borderColor: mobileColors.border,
      }}
    >
      {children}
    </View>
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
  onSelectHost: (host: string) => void;
  onChangeHost: (host: string) => void;
  onChangePin: (pin: string) => void;
  onPair: () => void;
}): React.JSX.Element {
  const insets = useSafeAreaInsets();
  const { width } = useWindowDimensions();
  const compact = width < 370;
  const wide = width >= 700;
  const [manual, setManual] = useState(false);
  const pinInputRef = useRef<TextInput>(null);
  const selected = found.find((hit) => hit.host === host.trim()) ?? null;
  const targetReady = manual ? host.trim().length > 0 : selected !== null;
  const canPair = targetReady && pin.length === 6 && !pairing;

  return (
    <View style={{ flex: 1, backgroundColor: mobileColors.background }} testID="pairing-screen">
      <ScrollView
        style={{ flex: 1 }}
        contentInsetAdjustmentBehavior="automatic"
        keyboardShouldPersistTaps="handled"
        showsVerticalScrollIndicator={false}
        contentContainerStyle={{
          minHeight: "100%",
          paddingTop: insets.top + (wide ? 24 : 16),
          paddingHorizontal: compact ? 16 : 20,
          paddingBottom: Math.max(insets.bottom, 20) + 24,
          width: "100%",
          maxWidth: 920,
          alignSelf: "center",
        }}
      >
        <View style={{ gap: wide ? 28 : 24 }}>
          <View style={{ gap: 18 }}>
            <Text
              selectable
              accessibilityRole="header"
              testID="remote-title"
              style={{ color: mobileColors.label, fontSize: 38, fontWeight: "700" }}
            >
              on-air
            </Text>
            <View style={{ maxWidth: 620, gap: 8 }}>
              <Text
                selectable
                testID="core-status"
                style={{ color: mobileColors.accent, fontSize: 15, fontWeight: "700" }}
              >
                Pair your Mac
              </Text>
              <Text
                selectable
                style={{
                  color: mobileColors.label,
                  fontSize: wide ? 34 : 30,
                  lineHeight: wide ? 40 : 36,
                  fontWeight: "700",
                }}
              >
                Connect this device to on-air
              </Text>
              <Text
                selectable
                style={{ color: mobileColors.secondaryLabel, fontSize: 17, lineHeight: 24 }}
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
              <Text selectable style={{ color: mobileColors.secondaryLabel, fontSize: 17 }}>
                Your Mac
              </Text>
              <SectionCard>
                {found.length > 0 ? (
                  found.map((hit, index) => {
                    const isSelected = !manual && selected?.host === hit.host;
                    return (
                      <Pressable
                        key={`${hit.host}:${hit.port}`}
                        accessibilityRole="button"
                        accessibilityState={{ selected: isSelected }}
                        onPress={() => {
                          setManual(false);
                          onSelectHost(hit.host);
                        }}
                        testID={`discovered-${hit.host}`}
                        style={({ pressed }) => ({
                          minHeight: 86,
                          paddingHorizontal: 16,
                          paddingVertical: 14,
                          flexDirection: "row",
                          alignItems: "center",
                          gap: 14,
                          borderBottomWidth: index === found.length - 1 ? 0 : 1,
                          borderBottomColor: mobileColors.separator,
                          backgroundColor: isSelected
                            ? "#2a1515"
                            : pressed
                              ? mobileColors.surfacePressed
                              : "transparent",
                        })}
                      >
                        <IconTile name={icons.computer} label="Mac" />
                        <View style={{ flex: 1, gap: 3 }}>
                          <Text
                            selectable
                            numberOfLines={1}
                            style={{ color: mobileColors.label, fontSize: 18, fontWeight: "700" }}
                          >
                            {hit.name ?? "on-air on Mac"}
                          </Text>
                          <Text
                            selectable
                            numberOfLines={1}
                            style={{ color: mobileColors.secondaryLabel, fontSize: 15 }}
                          >
                            {isSelected ? "Ready for your code" : "Nearby"}
                          </Text>
                        </View>
                        <NativeIcon
                          name={icons.check}
                          size={24}
                          color={isSelected ? mobileColors.accent : mobileColors.tertiaryLabel}
                          accessibilityLabel={isSelected ? "Selected" : "Choose this Mac"}
                        />
                      </Pressable>
                    );
                  })
                ) : (
                  <View
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
                      <ActivityIndicator size="large" color={mobileColors.accent} />
                    ) : (
                      <NativeIcon
                        name={icons.search}
                        size={34}
                        color={mobileColors.secondaryLabel}
                        accessibilityLabel="Mac not found"
                      />
                    )}
                    <View style={{ alignItems: "center", gap: 4 }}>
                      <Text
                        selectable
                        style={{ color: mobileColors.label, fontSize: 18, fontWeight: "700" }}
                      >
                        {scanning ? "Looking for your Mac…" : "No Mac found yet"}
                      </Text>
                      <Text
                        selectable
                        style={{
                          color: mobileColors.secondaryLabel,
                          fontSize: 15,
                          lineHeight: 20,
                          textAlign: "center",
                        }}
                      >
                        {scanning
                          ? "Make sure both devices are on the same Wi-Fi."
                          : "Open on-air on your Mac, then search again."}
                      </Text>
                    </View>
                  </View>
                )}
              </SectionCard>

              <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
                <Pressable
                  accessibilityRole="button"
                  accessibilityLabel="Search for Macs again"
                  disabled={scanning}
                  onPress={onScan}
                  testID="scan-button"
                  style={({ pressed }) => ({
                    minHeight: 44,
                    paddingHorizontal: 12,
                    alignItems: "center",
                    justifyContent: "center",
                    opacity: scanning ? 0.55 : pressed ? 0.65 : 1,
                  })}
                >
                  <Text
                    selectable
                    style={{ color: mobileColors.accent, fontSize: 16, fontWeight: "600" }}
                  >
                    {scanning ? "Searching…" : "Search again"}
                  </Text>
                </Pressable>
                <View style={{ width: 1, height: 18, backgroundColor: mobileColors.separator }} />
                <Pressable
                  accessibilityRole="button"
                  onPress={() => setManual((current) => !current)}
                  testID="manual-setup-button"
                  style={({ pressed }) => ({
                    minHeight: 44,
                    paddingHorizontal: 12,
                    alignItems: "center",
                    justifyContent: "center",
                    opacity: pressed ? 0.65 : 1,
                  })}
                >
                  <Text selectable style={{ color: mobileColors.secondaryLabel, fontSize: 16 }}>
                    {manual ? "Hide manual setup" : "Set up manually"}
                  </Text>
                </Pressable>
              </View>

              {manual ? (
                <View
                  testID="manual-setup"
                  style={{
                    padding: 16,
                    gap: 8,
                    borderRadius: 18,
                    borderCurve: "continuous",
                    backgroundColor: mobileColors.surface,
                    borderWidth: 1,
                    borderColor: mobileColors.border,
                  }}
                >
                  <Text
                    selectable
                    style={{ color: mobileColors.label, fontSize: 15, fontWeight: "600" }}
                  >
                    Mac network address
                  </Text>
                  <TextInput
                    value={host}
                    onChangeText={onChangeHost}
                    placeholder="192.168.1.20"
                    placeholderTextColor={mobileColors.tertiaryLabel}
                    autoCapitalize="none"
                    autoCorrect={false}
                    keyboardType="url"
                    returnKeyType="next"
                    blurOnSubmit={false}
                    onSubmitEditing={() => pinInputRef.current?.focus()}
                    testID="host-input"
                    style={{
                      minHeight: 50,
                      paddingHorizontal: 14,
                      color: mobileColors.label,
                      fontSize: 17,
                      borderRadius: 14,
                      borderCurve: "continuous",
                      backgroundColor: mobileColors.iconSurface,
                      borderWidth: 1,
                      borderColor: mobileColors.border,
                    }}
                  />
                </View>
              ) : null}
            </View>

            {targetReady ? (
              <View
                style={{ width: wide ? undefined : "100%", flex: wide ? 1 : undefined, gap: 10 }}
              >
                <Text selectable style={{ color: mobileColors.secondaryLabel, fontSize: 17 }}>
                  Pairing code
                </Text>
                <SectionCard>
                  <View style={{ padding: 18, gap: 18 }}>
                    <View style={{ flexDirection: "row", alignItems: "center", gap: 14 }}>
                      <IconTile name={icons.lock} label="Secure pairing" />
                      <View style={{ flex: 1, gap: 3 }}>
                        <Text
                          selectable
                          style={{ color: mobileColors.label, fontSize: 18, fontWeight: "700" }}
                        >
                          Enter the code on your Mac
                        </Text>
                        <Text
                          selectable
                          style={{
                            color: mobileColors.secondaryLabel,
                            fontSize: 15,
                            lineHeight: 20,
                          }}
                        >
                          We'll remember this Mac after you connect.
                        </Text>
                      </View>
                    </View>

                    <TextInput
                      ref={pinInputRef}
                      value={pin}
                      onChangeText={onChangePin}
                      placeholder="000000"
                      placeholderTextColor={mobileColors.tertiaryLabel}
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
                        color: mobileColors.label,
                        fontSize: compact ? 25 : 28,
                        fontWeight: "600",
                        letterSpacing: compact ? 8 : 11,
                        textAlign: "center",
                        borderRadius: 16,
                        borderCurve: "continuous",
                        backgroundColor: mobileColors.iconSurface,
                        borderWidth: 1,
                        borderColor: pin.length === 6 ? mobileColors.accent : mobileColors.border,
                      }}
                    />

                    {error ? (
                      <Text
                        selectable
                        accessibilityRole="alert"
                        style={{ color: "#ffb4ab", fontSize: 14, lineHeight: 19 }}
                      >
                        {error}
                      </Text>
                    ) : null}

                    <Pressable
                      accessibilityRole="button"
                      disabled={!canPair}
                      onPress={onPair}
                      testID="pair-button"
                      style={({ pressed }) => ({
                        minHeight: 54,
                        borderRadius: 17,
                        borderCurve: "continuous",
                        alignItems: "center",
                        justifyContent: "center",
                        backgroundColor: canPair ? mobileColors.accent : mobileColors.iconSurface,
                        opacity: pressed ? 0.7 : 1,
                      })}
                    >
                      {pairing ? (
                        <ActivityIndicator color="#ffffff" />
                      ) : (
                        <Text
                          selectable
                          style={{
                            color: canPair ? "#ffffff" : mobileColors.tertiaryLabel,
                            fontSize: 17,
                            fontWeight: "700",
                          }}
                        >
                          Connect
                        </Text>
                      )}
                    </Pressable>
                  </View>
                </SectionCard>
              </View>
            ) : null}
          </View>
        </View>
      </ScrollView>
    </View>
  );
}
