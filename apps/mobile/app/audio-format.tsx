import { FieldGroup, Picker, Row, Spacer, Text } from "@expo/ui";
import { useRemoteSession } from "@/remote-session";
import { SettingsForm } from "@/settings-form";

const rate = (hz: number) => `${hz / 1000} kHz`;
export default function AudioFormatScreen() {
  const {
    sampleRate,
    outputSampleRate,
    inputRates,
    outputRates,
    configuringRate,
    serviceAvailable,
    applyRate,
  } = useRemoteSession();
  const rows = [
    {
      kind: "input",
      label: "Input",
      value: sampleRate,
      rates: inputRates,
      testID: "sample-rate-picker",
    },
    {
      kind: "output",
      label: "Output",
      value: outputSampleRate,
      rates: outputRates,
      testID: "output-sample-rate-picker",
    },
  ] as const;
  return (
    <SettingsForm testID="audio-format-screen">
      <FieldGroup.Section title="Sample rate">
        {rows.map((row) => (
          <Row key={row.kind} alignment="center">
            <Text>{row.label}</Text>
            <Spacer />
            <Picker
              selectedValue={row.value}
              onValueChange={(v) => void applyRate(row.kind, Number(v))}
              appearance="menu"
              enabled={serviceAvailable && !configuringRate}
              testID={row.testID}
            >
              {row.rates.map((hz) => (
                <Picker.Item key={hz} label={rate(hz)} value={hz} />
              ))}
            </Picker>
          </Row>
        ))}
        <FieldGroup.SectionFooter>
          <Text>
            Keep the current rates unless your audio equipment requires a different format. Input is
            captured from the desktop; output is sent to the speaker.
          </Text>
        </FieldGroup.SectionFooter>
      </FieldGroup.Section>
    </SettingsForm>
  );
}
