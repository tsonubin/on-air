import { FieldGroup, Text } from "@expo/ui";
import { Stack } from "expo-router/stack";
import { NativeFader } from "@/native-fader";
import { useRemoteSession } from "@/remote-session";
import { SettingsForm } from "@/settings-form";

const bands = [
  { label: "Bass · 60 Hz", spoken: "Bass, 60 hertz" },
  { label: "Warmth · 250 Hz", spoken: "Warmth, 250 hertz" },
  { label: "Midrange · 1 kHz", spoken: "Midrange, 1 kilohertz" },
  { label: "Presence · 4 kHz", spoken: "Presence, 4 kilohertz" },
  { label: "Air · 12 kHz", spoken: "Air, 12 kilohertz" },
];

export default function EqualizerScreen() {
  const { gains, changeBand, applyEq, view } = useRemoteSession();
  const flat = gains.every((g) => g === 0);
  return (
    <>
      <SettingsForm testID="equalizer-screen">
        <FieldGroup.Section title="Five-band equalizer">
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
            <Text>
              Move left to reduce a band, or right to boost it. 0 dB leaves the sound unchanged.
              Changes are applied as you adjust.
            </Text>
          </FieldGroup.SectionFooter>
        </FieldGroup.Section>
      </SettingsForm>
      <Stack.Toolbar placement="right">
        <Stack.Toolbar.Button
          disabled={flat || !view.controlsEnabled}
          onPress={() => applyEq([0, 0, 0, 0, 0])}
        >
          Reset
        </Stack.Toolbar.Button>
      </Stack.Toolbar>
    </>
  );
}
