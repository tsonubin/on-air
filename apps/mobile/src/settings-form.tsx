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
import { FormText, sectionTitle } from "@/ui/form-text";

const ios = Platform.OS === "ios";

/** Row background for every section of a settings form (iOS; Android follows the seeded host). */
export const sectionModifiers = ios ? [listRowBackground(colors.surface)] : undefined;

/**
 * Colors for a ListItem placed in a section. Android's section already wraps
 * each row in a Material ListItem, so a clear container keeps a nested one from
 * painting an inset box (iOS ignores ListItem colors).
 */
export const rowColors = { containerColor: "transparent" } as const;

// FieldGroup is a native, scrollable SwiftUI Form on iOS. Native stack headers
// supply the safe area; the form owns scrolling and Dynamic Type layout. The
// system's grouped gray is hidden so the form sits on the faceplate well.
export function SettingsForm({ children, testID }: { children: ReactNode; testID: string }) {
  const { error, warning, view } = useRemoteSession();
  const message = error ?? warning;
  return (
    <Host style={{ flex: 1 }} colorScheme="dark" seedColor={colors.amber}>
      <FieldGroup
        testID={testID}
        modifiers={
          ios ? [scrollContentBackground("hidden"), background(colors.background)] : undefined
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
  );
}
