import { FieldGroup, ListItem, Text } from "@expo/ui";
import { router } from "expo-router";
import { useRemoteSession } from "@/remote-session";
import { SettingsForm } from "@/settings-form";

export default function SoundScreen() {
  const { gains } = useRemoteSession();
  return (
    <SettingsForm testID="sound-screen">
      <FieldGroup.Section title="Tone">
        <ListItem
          onPress={() => router.push("/equalizer")}
          supportingText="Adjust bass, vocals, and treble"
          testID="equalizer-link"
        >
          <Text>Equalizer</Text>
          <ListItem.Trailing>
            <Text>{gains.every((g) => g === 0) ? "Flat ›" : "Custom ›"}</Text>
          </ListItem.Trailing>
        </ListItem>
        <FieldGroup.SectionFooter>
          <Text>Sound changes apply to the connected desktop.</Text>
        </FieldGroup.SectionFooter>
      </FieldGroup.Section>
      <FieldGroup.Section title="Advanced">
        <ListItem
          onPress={() => router.push("/audio-format")}
          supportingText="Input and output sample rates"
          testID="audio-format-link"
        >
          <Text>Audio format</Text>
          <ListItem.Trailing>
            <Text>›</Text>
          </ListItem.Trailing>
        </ListItem>
      </FieldGroup.Section>
    </SettingsForm>
  );
}
