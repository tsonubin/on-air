import { Button, FieldGroup } from "@expo/ui";
import { useRemoteSession } from "@/remote-session";
import { SettingsForm, sectionModifiers } from "@/settings-form";
import { FormText, SectionTitle } from "@/ui/form-text";

export default function ConnectionScreen() {
  const { pairedHost, view, refreshing, refresh, disconnect } = useRemoteSession();
  return (
    <SettingsForm testID="connection-screen">
      <FieldGroup.Section modifiers={sectionModifiers}>
        <SectionTitle title="Paired desktop" />
        <FormText variant="value" copyable testID="paired-host">
          {pairedHost}
        </FormText>
        <FormText variant="body">{view.connectionText}</FormText>
        <FieldGroup.SectionFooter>
          <FormText variant="footnote">
            Your PIN pairing is saved on this phone. Keep both devices on the same Wi-Fi.
          </FormText>
        </FieldGroup.SectionFooter>
      </FieldGroup.Section>
      <FieldGroup.Section modifiers={sectionModifiers}>
        <Button
          label={refreshing ? "Refreshing…" : "Refresh connection"}
          disabled={refreshing}
          onPress={() => void refresh()}
          testID="refresh-button"
        />
        <FieldGroup.SectionFooter>
          <FormText variant="footnote">
            Refresh reloads the mixer without removing your pairing. If the desktop address changed,
            forget it and pair again.
          </FormText>
        </FieldGroup.SectionFooter>
      </FieldGroup.Section>
      <FieldGroup.Section modifiers={sectionModifiers}>
        <Button
          label="Forget desktop"
          variant="text"
          onPress={disconnect}
          testID="disconnect-menu-button"
        />
      </FieldGroup.Section>
    </SettingsForm>
  );
}
