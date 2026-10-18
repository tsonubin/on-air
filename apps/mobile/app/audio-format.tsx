import { Button, FieldGroup, Picker, Row, Spacer, Text } from "@expo/ui";
import type { DspQuality } from "@on-air/api-types";
import {
  AUDIO_CHECK_DESCRIPTION,
  AUDIO_CHECK_OVER_BUDGET,
  audioBenchmarkSummary,
  CAPTURE_BUFFER_OPTIONS,
  captureBufferLabel,
  DSP_QUALITY_COSTS,
  DSP_QUALITY_DESCRIPTION,
  DSP_QUALITY_LABELS,
  DSP_QUALITY_OPTIONS,
} from "@on-air/control-client";
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
    view,
    applyRate,
    captureBufferMs,
    audioPerformance,
    audioQuality,
    applyQuality,
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
      <FieldGroup.Section title="Sample rate">
        {rows.map((row) => (
          <Row key={row.kind} alignment="center">
            <Text>{row.label}</Text>
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
          <Text>
            Keep the current rates unless your audio equipment requires a different format. Input is
            captured from the desktop; output is sent to the speaker.
          </Text>
        </FieldGroup.SectionFooter>
      </FieldGroup.Section>
      <FieldGroup.Section title="Audio performance">
        <Row alignment="center">
          <Text>Requested capture buffer</Text>
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
        <Row alignment="center">
          <Text>DSP quality</Text>
          <Spacer />
          <Picker
            selectedValue={audioQuality?.quality}
            appearance="menu"
            enabled={view.controlsEnabled && audioQuality !== null && !audioPerformance.busy}
            onValueChange={(value) => void applyQuality(value as DspQuality)}
            testID="dsp-quality-picker"
          >
            {DSP_QUALITY_OPTIONS.map((profile) => (
              <Picker.Item
                key={profile}
                label={`${DSP_QUALITY_LABELS[profile]}${
                  audioQuality?.recommended === profile ? " (recommended)" : ""
                }`}
                value={profile}
              />
            ))}
          </Picker>
        </Row>
        <Text>
          {DSP_QUALITY_DESCRIPTION} {DSP_QUALITY_COSTS[audioQuality?.quality ?? "balanced"]}
        </Text>
        <Button
          label={audioPerformance.busy ? "Working…" : "Run audio check"}
          disabled={!view.controlsEnabled || captureBufferMs === undefined || audioPerformance.busy}
          onPress={() => void audioPerformance.run()}
          testID="audio-check"
        />
        {report && (
          <>
            <Text>{audioBenchmarkSummary(report)}</Text>
            {!report.within_buffer_budget && <Text>{AUDIO_CHECK_OVER_BUDGET}</Text>}
            <Button
              label={`Apply recommendation (${report.recommended_buffer_ms} ms)`}
              disabled={!view.controlsEnabled || audioPerformance.busy}
              onPress={() => void audioPerformance.apply(report.recommended_buffer_ms)}
              testID="apply-audio-buffer"
            />
            <Button
              label={`Apply recommended quality (${
                DSP_QUALITY_LABELS[report.recommended_quality]
              })`}
              disabled={!view.controlsEnabled || audioPerformance.busy}
              onPress={() => void applyQuality(report.recommended_quality)}
              testID="apply-dsp-quality"
            />
          </>
        )}
        {audioPerformance.error && <Text>{audioPerformance.error}</Text>}
        <FieldGroup.SectionFooter>
          <Text>{AUDIO_CHECK_DESCRIPTION}</Text>
          {captureBufferMs === undefined && (
            <Text>Update the on-air core to use these settings.</Text>
          )}
        </FieldGroup.SectionFooter>
      </FieldGroup.Section>
    </SettingsForm>
  );
}
