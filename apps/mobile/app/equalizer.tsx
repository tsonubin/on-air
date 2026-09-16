import { FieldGroup, Text } from "@expo/ui";
import { Stack } from "expo-router/stack";
import { NativeFader } from "@/native-fader";
import { useRemoteSession } from "@/remote-session";
import { SettingsForm } from "@/settings-form";

const bands = [
  "Bass · 60 Hz",
  "Warmth · 250 Hz",
  "Midrange · 1 kHz",
  "Presence · 4 kHz",
  "Air · 12 kHz",
];

export default function EqualizerScreen() {
  const { gains, changeBand, applyEq, serviceAvailable } = useRemoteSession();
  const flat = gains.every((g) => g === 0);
  return (
    <>
      <SettingsForm testID="equalizer-screen">
        <FieldGroup.Section title="Five-band equalizer">
          {gains.map((gain, index) => (
            <NativeFader
              key={bands[index]}
              label={bands[index]}
              value={gain}
              min={-12}
              max={12}
              step={0.5}
              unit="dB"
              signed
              testId={`eq-band-${index}`}
              disabled={!serviceAvailable}
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
          disabled={flat || !serviceAvailable}
          onPress={() => applyEq([0, 0, 0, 0, 0])}
        >
          Reset
        </Stack.Toolbar.Button>
      </Stack.Toolbar>
    </>
  );
}
