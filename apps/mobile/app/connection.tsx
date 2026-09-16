import { Button, FieldGroup, Text } from "@expo/ui";
import { useRemoteSession } from "@/remote-session";
import { SettingsForm } from "@/settings-form";

export default function ConnectionScreen() {
  const { host, connectionState, refreshing, refresh, disconnect } = useRemoteSession();
  return (
    <SettingsForm testID="connection-screen">
      <FieldGroup.Section title="Paired desktop">
        <Text testID="paired-token">{host}</Text>
        <Text>
          {connectionState === "connected"
            ? "Connected to desktop"
            : connectionState === "connecting"
              ? "Connecting…"
              : "Reconnecting…"}
        </Text>
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
