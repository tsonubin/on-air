import { Button, FieldGroup, Text } from "@expo/ui";
import { textSelection } from "@expo/ui/swift-ui/modifiers";
import { Platform } from "react-native";
import { useRemoteSession } from "@/remote-session";
import { SettingsForm } from "@/settings-form";

export default function ConnectionScreen() {
  const { pairedHost, view, refreshing, refresh, disconnect } = useRemoteSession();
  return (
    <SettingsForm testID="connection-screen">
      <FieldGroup.Section title="Paired desktop">
        <Text
          testID="paired-host"
          modifiers={Platform.OS === "ios" ? [textSelection(true)] : undefined}
        >
          {pairedHost}
        </Text>
        <Text>{view.connectionText}</Text>
        <FieldGroup.SectionFooter>
          <Text>Your PIN pairing is saved on this phone. Keep both devices on the same Wi-Fi.</Text>
        </FieldGroup.SectionFooter>
      </FieldGroup.Section>
      <FieldGroup.Section>
        <Button
          label={refreshing ? "Refreshing…" : "Refresh connection"}
          disabled={refreshing}
          onPress={() => void refresh()}
          testID="refresh-button"
        />
        <FieldGroup.SectionFooter>
          <Text>
            Refresh reloads the mixer without removing your pairing. If the desktop address changed,
            forget it and pair again.
          </Text>
        </FieldGroup.SectionFooter>
      </FieldGroup.Section>
      <FieldGroup.Section>
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
