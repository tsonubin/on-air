import { FieldGroup, Host, Text } from "@expo/ui";
import { StatusBar } from "expo-status-bar";
import type { ReactNode } from "react";
import { useColorScheme } from "react-native";
import { useRemoteSession } from "@/remote-session";

// FieldGroup is a native, scrollable SwiftUI Form on iOS. Native stack headers
// supply the safe area; the form owns scrolling and Dynamic Type layout.
export function SettingsForm({ children, testID }: { children: ReactNode; testID: string }) {
  const scheme = useColorScheme() ?? "light";
  const { error, servicePaused } = useRemoteSession();
  return (
    <>
      <StatusBar style={scheme === "dark" ? "light" : "dark"} />
      <Host style={{ flex: 1 }} colorScheme={scheme}>
        <FieldGroup testID={testID}>
          {error && (
            <FieldGroup.Section title="Connection">
              <Text>{error}</Text>
            </FieldGroup.Section>
          )}
          {servicePaused && (
            <FieldGroup.Section>
              <Text>
                The desktop service is paused. Turn it on from the desktop to change sound settings.
              </Text>
            </FieldGroup.Section>
          )}
          {children}
        </FieldGroup>
      </Host>
    </>
  );
}
