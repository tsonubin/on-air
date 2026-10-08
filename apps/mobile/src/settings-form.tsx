import { FieldGroup, Host, Text } from "@expo/ui";
import type { ReactNode } from "react";
import { useRemoteSession } from "@/remote-session";

// FieldGroup is a native, scrollable SwiftUI Form on iOS. Native stack headers
// supply the safe area; the form owns scrolling and Dynamic Type layout.
export function SettingsForm({ children, testID }: { children: ReactNode; testID: string }) {
  const { error, warning, view } = useRemoteSession();
  const message = error ?? warning;
  return (
    <Host style={{ flex: 1 }} colorScheme="dark">
      <FieldGroup testID={testID}>
        {message && (
          <FieldGroup.Section title="Connection">
            <Text>{message}</Text>
          </FieldGroup.Section>
        )}
        {view.paused && (
          <FieldGroup.Section>
            <Text>
              The desktop service is paused. Turn it on from the desktop to change sound settings.
            </Text>
          </FieldGroup.Section>
        )}
        {children}
      </FieldGroup>
    </Host>
  );
}
