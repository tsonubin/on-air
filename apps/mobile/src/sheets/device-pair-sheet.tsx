import {
  BottomSheet,
  Button,
  Column,
  Row,
  Spacer,
  Text,
  TextInput,
  useNativeState,
} from "@expo/ui";
import { useRef } from "react";
import {
  DEVICE_PIN_LENGTH,
  type DevicePairTarget,
  digitsOnly,
  useRemoteSession,
} from "@/remote-session";
import { colors, radius, theme } from "@/theme";
import { sheetProps } from "./sheet-props";

export function DevicePairSheet({
  target,
  onClose,
}: {
  target: DevicePairTarget;
  onClose: () => void;
}) {
  const { devicePairing, pairDevice } = useRemoteSession();
  const pinInput = useNativeState("");
  // The native field owns its text; JS keeps the sanitised copy it submits.
  const pin = useRef("");
  const airplay = target.transport === "airplay";

  const submit = async () => {
    if (await pairDevice(target, pin.current)) onClose();
  };

  return (
    <BottomSheet
      {...sheetProps({ kind: "device-pair", transport: target.transport })}
      isPresented
      onDismiss={onClose}
      testID="pair-sheet"
    >
      <Column spacing={theme.spacing.md} style={{ padding: theme.spacing.md }}>
        <Text textStyle={{ fontSize: 22, fontWeight: "700" }}>{`Pair ${target.name}`}</Text>
        <Text>
          {airplay
            ? "Enter the code shown by the speaker. If no code appears, leave it blank."
            : "Confirm pairing on the speaker or in the computer’s Bluetooth settings, then continue."}
        </Text>
        {airplay && (
          <TextInput
            value={pinInput}
            onChangeText={(value) => {
              const next = digitsOnly(value, DEVICE_PIN_LENGTH);
              pin.current = next;
              pinInput.value = next;
            }}
            placeholder="Speaker code"
            keyboardType="number-pad"
            inputMode="numeric"
            maxLength={DEVICE_PIN_LENGTH}
            testID="device-pin-input"
            style={{
              padding: 12,
              borderWidth: 1,
              borderColor: colors.accent,
              borderRadius: radius.sm,
            }}
          />
        )}
        <Row alignment="center" spacing={theme.spacing.sm}>
          <Button
            label="Cancel"
            variant="text"
            disabled={devicePairing}
            onPress={onClose}
            testID="device-pair-cancel"
          />
          <Spacer />
          <Button
            label={devicePairing ? "Pairing…" : "Pair and connect"}
            disabled={devicePairing}
            onPress={() => void submit()}
            testID="device-pair-submit"
          />
        </Row>
      </Column>
    </BottomSheet>
  );
}
