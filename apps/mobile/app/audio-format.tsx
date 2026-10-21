import { Button, FieldGroup, Picker, Row, Spacer } from "@expo/ui";
import {
  AUDIO_CHECK_DESCRIPTION,
  AUDIO_CHECK_OVER_BUDGET,
  audioBenchmarkSummary,
  CAPTURE_BUFFER_OPTIONS,
  captureBufferLabel,
} from "@on-air/control-client";
import { useRemoteSession } from "@/remote-session";
import { SettingsForm, sectionModifiers } from "@/settings-form";
import { FormText, SectionTitle } from "@/ui/form-text";

const rate = (hz: number) => `${hz / 1000} kHz`;
export default function AudioFormatScreen() {
  const {
    sampleRate,
    outputSampleRate,
    inputRates,
    outputRates,
    configuringRate,
    view,
    applyRate,
    captureBufferMs,
    audioPerformance,
  } = useRemoteSession();
  const report = audioPerformance.report;
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
      <FieldGroup.Section modifiers={sectionModifiers}>
        <SectionTitle title="Sample rate" />
        {rows.map((row) => (
          <Row key={row.kind} alignment="center">
            <FormText variant="body">{row.label}</FormText>
            <Spacer />
            <Picker
              selectedValue={row.value}
              onValueChange={(v) => void applyRate(row.kind, Number(v))}
              appearance="menu"
              enabled={view.controlsEnabled && !configuringRate}
              testID={row.testID}
            >
              {row.rates.map((hz) => (
                <Picker.Item key={hz} label={rate(hz)} value={hz} />
              ))}
            </Picker>
          </Row>
        ))}
        <FieldGroup.SectionFooter>
          <FormText variant="footnote">
            Keep the current rates unless your audio equipment requires a different format. Input is
            captured from the desktop; output is sent to the speaker.
          </FormText>
        </FieldGroup.SectionFooter>
      </FieldGroup.Section>
      <FieldGroup.Section modifiers={sectionModifiers}>
        <SectionTitle title="Audio performance" />
        <Row alignment="center">
          <FormText variant="body">Requested capture buffer</FormText>
          <Spacer />
          <Picker
            selectedValue={captureBufferMs ?? 0}
            appearance="menu"
            enabled={
              view.controlsEnabled && captureBufferMs !== undefined && !audioPerformance.busy
            }
            onValueChange={(value) => void audioPerformance.apply(Number(value))}
            testID="capture-buffer-picker"
          >
            {CAPTURE_BUFFER_OPTIONS.map((ms) => (
              <Picker.Item key={ms} label={captureBufferLabel(ms)} value={ms} />
            ))}
          </Picker>
        </Row>
        <Button
          label={audioPerformance.busy ? "Working…" : "Run audio check"}
          disabled={!view.controlsEnabled || captureBufferMs === undefined || audioPerformance.busy}
          onPress={() => void audioPerformance.run()}
          testID="audio-check"
        />
        {report && (
          <>
            <FormText variant="body">{audioBenchmarkSummary(report)}</FormText>
            {!report.within_buffer_budget && (
              <FormText variant="body">{AUDIO_CHECK_OVER_BUDGET}</FormText>
            )}
            <Button
              label={`Apply recommendation (${report.recommended_buffer_ms} ms)`}
              disabled={!view.controlsEnabled || audioPerformance.busy}
              onPress={() => void audioPerformance.apply(report.recommended_buffer_ms)}
              testID="apply-audio-buffer"
            />
          </>
        )}
        {audioPerformance.error && <FormText variant="body">{audioPerformance.error}</FormText>}
        <FieldGroup.SectionFooter>
          <FormText variant="footnote">{AUDIO_CHECK_DESCRIPTION}</FormText>
          {captureBufferMs === undefined && (
            <FormText variant="footnote">Update the on-air core to use these settings.</FormText>
          )}
        </FieldGroup.SectionFooter>
      </FieldGroup.Section>
    </SettingsForm>
  );
}
