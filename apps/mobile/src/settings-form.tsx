import { FieldGroup, Host } from "@expo/ui";
import {
  background,
  listRowBackground,
  scrollContentBackground,
} from "@expo/ui/swift-ui/modifiers";
import type { ReactNode } from "react";
import { Platform } from "react-native";
import { useRemoteSession } from "@/remote-session";
import { colors } from "@/theme";
import { Faceplate } from "@/ui/faceplate";
import { FormText, sectionTitle } from "@/ui/form-text";

const ios = Platform.OS === "ios";

/**
 * Row background for every section of a settings form (iOS; Android follows the
 * seeded host): translucent, so the faceplate's grain shows through the rows.
 */
export const sectionModifiers = ios ? [listRowBackground(colors.faceRow)] : undefined;

// FieldGroup is a native, scrollable SwiftUI Form on iOS and a Compose list on
// Android. Native stack headers supply the safe area; the form owns scrolling
// and Dynamic Type layout. Both platforms' own backgrounds are cleared so the
// form sits on the satin faceplate drawn behind it.
export function SettingsForm({ children, testID }: { children: ReactNode; testID: string }) {
  const { error, warning, view } = useRemoteSession();
  const message = error ?? warning;
  return (
    <Faceplate>
      <Host
        style={{ flex: 1, backgroundColor: "transparent" }}
        colorScheme="dark"
        seedColor={colors.amber}
      >
        <FieldGroup
          testID={testID}
          // Compose paints the Material surface unless the list's background is overridden.
          style={Platform.OS === "android" ? { backgroundColor: "transparent" } : undefined}
          modifiers={
            ios ? [scrollContentBackground("hidden"), background("transparent")] : undefined
          }
        >
          {message && (
            <FieldGroup.Section modifiers={sectionModifiers}>
              {sectionTitle("Connection")}
              <FormText variant="body">{message}</FormText>
            </FieldGroup.Section>
          )}
          {view.paused && (
            <FieldGroup.Section modifiers={sectionModifiers}>
              <FormText variant="body">
                The desktop service is paused. Turn it on from the desktop to change sound settings.
              </FormText>
            </FieldGroup.Section>
          )}
          {children}
        </FieldGroup>
      </Host>
    </Faceplate>
  );
}
