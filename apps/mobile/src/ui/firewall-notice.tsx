import { Button, Host } from "@expo/ui";
import type { FirewallWarning } from "@on-air/api-types";
import { useState } from "react";
import { Text, View } from "react-native";
import { colors, radius } from "@/theme";

export function FirewallNotice({ warning }: { warning?: FirewallWarning | null }) {
  const [expanded, setExpanded] = useState(false);
  if (!warning) return null;
  return (
    <View
      testID="firewall-warning"
      style={{
        padding: 14,
        gap: 10,
        borderRadius: radius.md,
        backgroundColor: colors.errorSurface,
        borderColor: colors.errorBorder,
        borderWidth: 1,
      }}
    >
      <Text accessibilityRole="alert" style={{ color: colors.onError, fontSize: 15 }}>
        {warning.message}
      </Text>
      <Host matchContents colorScheme="dark" seedColor={colors.amber}>
        <Button
          label={expanded ? "Hide firewall help" : "Firewall help"}
          variant="text"
          onPress={() => setExpanded(!expanded)}
        />
      </Host>
      {expanded && (
        <>
          <Text style={{ color: colors.onError }}>
            On the computer running on-air, open Firewall help and choose Allow LAN audio.
            Administrator authentication happens on that computer.
          </Text>
          <Text style={{ color: colors.onError }}>
            This allows {warning.subnet} to reach TCP port {warning.port}. You can also run these
            commands on that computer:
          </Text>
          <Text
            selectable
            testID="firewall-command"
            style={{ color: colors.onError, fontSize: 13 }}
          >
            {warning.repair_command}
          </Text>
        </>
      )}
    </View>
  );
}
