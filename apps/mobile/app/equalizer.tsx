import { Stack } from "expo-router/stack";
import { Platform, Pressable, Text, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { EqPanel } from "@/eq-panel";
import { useRemoteSession } from "@/remote-session";
import { colors, layout, type } from "@/theme";
import { Faceplate } from "@/ui/faceplate";

/** Android's header Reset: an amber key caption, dimmed while there is nothing to reset. */
function HeaderReset({ disabled, onPress }: { disabled: boolean; onPress: () => void }) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel="Reset equalizer"
      accessibilityState={{ disabled }}
      disabled={disabled}
      onPress={onPress}
      testID="eq-reset"
      style={({ pressed }) => ({
        minHeight: layout.minTarget,
        justifyContent: "center",
        opacity: pressed ? 0.65 : 1,
      })}
    >
      <Text
        style={[
          type.label,
          { fontSize: 13, color: disabled ? colors.tertiaryLabel : colors.amber },
        ]}
      >
        Reset
      </Text>
    </Pressable>
  );
}

export default function EqualizerScreen() {
  const { gains, changeBand, applyEq, view } = useRemoteSession();
  const insets = useSafeAreaInsets();
  const resetDisabled = gains.every((g) => g === 0) || !view.controlsEnabled;
  const reset = () => applyEq([0, 0, 0, 0, 0]);
  return (
    <>
      <Faceplate testID="equalizer-screen">
        {/* The native header covers the top; in landscape the sides clear the sensor housing. */}
        <View
          testID="equalizer-body"
          style={{
            flex: 1,
            paddingLeft: Math.max(insets.left, 16),
            paddingRight: Math.max(insets.right, 16),
            paddingTop: 16,
            paddingBottom: Math.max(insets.bottom, 16),
          }}
        >
          <EqPanel
            gains={gains}
            disabled={!view.controlsEnabled}
            onChangeBand={(index, value) => changeBand(index, value)}
          />
        </View>
      </Faceplate>
      {Platform.OS === "android" ? (
        // Android's toolbar buttons draw only an image icon, so Reset is a header caption there.
        <Stack.Screen
          options={{ headerRight: () => <HeaderReset disabled={resetDisabled} onPress={reset} /> }}
        />
      ) : (
        // An icon as well as a title: the folded iPhone Duo moves only bar items with one into its rail.
        <Stack.Toolbar placement="right">
          <Stack.Toolbar.Button
            icon="arrow.counterclockwise"
            accessibilityLabel="Reset equalizer"
            disabled={resetDisabled}
            onPress={reset}
          >
            Reset
          </Stack.Toolbar.Button>
        </Stack.Toolbar>
      )}
    </>
  );
}
