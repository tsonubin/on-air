import { useState } from "react";
import { Text, View } from "react-native";
import {
  DEVICE_PIN_LENGTH,
  type DevicePairTarget,
  digitsOnly,
  useRemoteSession,
} from "@/remote-session";
import { colors, type } from "@/theme";
import { LedCodeInput } from "@/ui/led-code-input";
import { PanelKey } from "@/ui/panel-key";
import { SheetPanel } from "./sheet-panel";

/**
 * Pairing a speaker that asks first: AirPlay shows a code to type into the LED
 * field (or none, when it is left blank); Bluetooth confirms on the speaker.
 */
export function DevicePairSheet({
  target,
  onClose,
}: {
  target: DevicePairTarget;
  onClose: () => void;
}) {
  const { devicePairing, pairDevice } = useRemoteSession();
  const [pin, setPin] = useState("");
  const airplay = target.transport === "airplay";

  const submit = async () => {
    if (await pairDevice(target, pin)) onClose();
  };

  return (
    <SheetPanel
      spec={{ kind: "device-pair", transport: target.transport }}
      caption="Pair speaker"
      done={false}
      onClose={onClose}
      testID="pair-sheet"
    >
      <View style={{ gap: 4 }}>
        <Text
          maxFontSizeMultiplier={1.3}
          style={[type.title, { fontSize: 18, color: colors.label }]}
        >
          {target.name}
        </Text>
        <Text
          maxFontSizeMultiplier={1.3}
          style={[type.body, { fontSize: 12, lineHeight: 16, color: colors.secondaryLabel }]}
        >
          {airplay
            ? "Enter the code the speaker shows. No code? Leave it blank."
            : "Confirm pairing on the speaker or in the computer’s Bluetooth settings, then continue."}
        </Text>
      </View>
      {airplay ? (
        <LedCodeInput
          length={DEVICE_PIN_LENGTH}
          value={pin}
          onChangeText={(value) => setPin(digitsOnly(value, DEVICE_PIN_LENGTH))}
          editable={!devicePairing}
          accessibilityLabel="Speaker code"
          testID="device-pin-input"
        />
      ) : null}
      <View style={{ flexDirection: "row", gap: 8 }}>
        <PanelKey
          label="Cancel"
          disabled={devicePairing}
          onPress={onClose}
          testID="device-pair-cancel"
          style={{ flex: 1, minHeight: 44 }}
        />
        <PanelKey
          label={devicePairing ? "Pairing…" : "Pair and connect"}
          lit
          disabled={devicePairing}
          onPress={() => void submit()}
          testID="device-pair-submit"
          style={{ flex: 2, minHeight: 44 }}
        />
      </View>
    </SheetPanel>
  );
}
