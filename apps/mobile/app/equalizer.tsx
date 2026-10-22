import { FieldGroup } from "@expo/ui";
import { Stack } from "expo-router/stack";
import { Platform, Pressable, Text } from "react-native";
import { NativeFader } from "@/native-fader";
import { useRemoteSession } from "@/remote-session";
import { SettingsForm, sectionModifiers } from "@/settings-form";
import { colors, layout, type } from "@/theme";
import { FormText, sectionTitle } from "@/ui/form-text";

const bands = [
  { label: "Bass · 60 Hz", spoken: "Bass, 60 hertz" },
  { label: "Warmth · 250 Hz", spoken: "Warmth, 250 hertz" },
  { label: "Midrange · 1 kHz", spoken: "Midrange, 1 kilohertz" },
  { label: "Presence · 4 kHz", spoken: "Presence, 4 kilohertz" },
  { label: "Air · 12 kHz", spoken: "Air, 12 kilohertz" },
];

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
  const resetDisabled = gains.every((g) => g === 0) || !view.controlsEnabled;
  const reset = () => applyEq([0, 0, 0, 0, 0]);
  return (
    <>
      <SettingsForm testID="equalizer-screen">
        <FieldGroup.Section modifiers={sectionModifiers}>
          {sectionTitle("Five-band equalizer")}
          {gains.map((gain, index) => (
            <NativeFader
              key={bands[index].label}
              label={bands[index].label}
              accessibilityLabel={bands[index].spoken}
              value={gain}
              min={-12}
              max={12}
              step={0.5}
              unit="dB"
              signed
              testId={`eq-band-${index}`}
              disabled={!view.controlsEnabled}
              onChange={(value) => changeBand(index, value)}
            />
          ))}
          <FieldGroup.SectionFooter>
            <FormText variant="footnote">
              Move left to reduce a band, or right to boost it. 0 dB leaves the sound unchanged.
              Changes are applied as you adjust.
            </FormText>
          </FieldGroup.SectionFooter>
        </FieldGroup.Section>
      </SettingsForm>
      {Platform.OS === "android" ? (
        // Android's toolbar buttons draw only an image icon, so Reset is a header caption there.
        <Stack.Screen
          options={{ headerRight: () => <HeaderReset disabled={resetDisabled} onPress={reset} /> }}
        />
      ) : (
        <Stack.Toolbar placement="right">
          <Stack.Toolbar.Button disabled={resetDisabled} onPress={reset}>
            Reset
          </Stack.Toolbar.Button>
        </Stack.Toolbar>
      )}
    </>
  );
}
