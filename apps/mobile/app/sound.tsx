import { FieldGroup, ListItem } from "@expo/ui";
import { router } from "expo-router";
import { useRemoteSession } from "@/remote-session";
import { rowColors, SettingsForm, sectionModifiers } from "@/settings-form";
import { FormText, sectionTitle } from "@/ui/form-text";

export default function SoundScreen() {
  const { gains } = useRemoteSession();
  return (
    <SettingsForm testID="sound-screen">
      <FieldGroup.Section modifiers={sectionModifiers}>
        {sectionTitle("Tone")}
        <ListItem
          colors={rowColors}
          onPress={() => router.push("/equalizer")}
          supportingText="Adjust bass, vocals, and treble"
          testID="equalizer-link"
        >
          <FormText variant="body">Equalizer</FormText>
          <ListItem.Trailing>
            <FormText variant="value">
              {gains.every((g) => g === 0) ? "Flat ›" : "Custom ›"}
            </FormText>
          </ListItem.Trailing>
        </ListItem>
        <FieldGroup.SectionFooter>
          <FormText variant="footnote">Sound changes apply to the connected desktop.</FormText>
        </FieldGroup.SectionFooter>
      </FieldGroup.Section>
      <FieldGroup.Section modifiers={sectionModifiers}>
        {sectionTitle("Advanced")}
        <ListItem
          colors={rowColors}
          onPress={() => router.push("/audio-format")}
          supportingText="Input and output sample rates"
          testID="audio-format-link"
        >
          <FormText variant="body">Audio format</FormText>
          <ListItem.Trailing>
            <FormText variant="value">›</FormText>
          </ListItem.Trailing>
        </ListItem>
      </FieldGroup.Section>
    </SettingsForm>
  );
}
