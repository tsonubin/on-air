import { BottomSheet, Button, Column, Picker, Row, Spacer, Text } from "@expo/ui";
import { prettyInput } from "@/pretty-input";
import { useRemoteSession } from "@/remote-session";
import { theme } from "@/theme";
import { sheetProps } from "./sheet-props";

export function SourceSheet({ onClose }: { onClose: () => void }) {
  const { inputs, activeInput, busyTarget, view, pickInput } = useRemoteSession();
  return (
    <BottomSheet
      {...sheetProps({ kind: "source" })}
      isPresented
      onDismiss={onClose}
      testID="source-sheet"
    >
      <Column spacing={theme.spacing.md} style={{ padding: theme.spacing.md }}>
        <Row alignment="center" spacing={theme.spacing.sm}>
          <Text textStyle={{ fontSize: 22, fontWeight: "700" }}>Choose source</Text>
          <Spacer />
          <Button label="Done" variant="text" onPress={onClose} />
        </Row>
        <Text>Select the Mac audio capture device to stream.</Text>
        <Picker
          selectedValue={activeInput}
          onValueChange={async (name) => {
            if (await pickInput(String(name))) onClose();
          }}
          appearance="menu"
          enabled={view.controlsEnabled && busyTarget === null && inputs.length > 0}
          testID="source-picker"
        >
          <Picker.Item label="Choose a source" value="" />
          {inputs.map((name) => (
            <Picker.Item key={name} label={prettyInput(name)} value={name} />
          ))}
        </Picker>
        {inputs.length === 0 && <Text>No capture devices are available.</Text>}
      </Column>
    </BottomSheet>
  );
}
