import { BottomSheet, RNHostView } from "@expo/ui";
import type { ReactNode } from "react";
import { Platform, Text, View } from "react-native";
import { colors, type } from "@/theme";
import { Faceplate } from "@/ui/faceplate";
import { PanelKey } from "@/ui/panel-key";
import { type SheetSpec, sheetProps } from "./sheet-props";

/**
 * A bottom sheet cut from the same faceplate as the mixer: the native sheet
 * presents and dismisses it, and the faceplate fills it edge to edge under a
 * silkscreened caption and, except where the sheet has its own Cancel, a Done key.
 */
export function SheetPanel({
  spec,
  caption,
  onClose,
  done = true,
  testID,
  children,
}: {
  spec: SheetSpec;
  caption: string;
  onClose: () => void;
  done?: boolean;
  testID: string;
  children: ReactNode;
}) {
  return (
    <BottomSheet {...sheetProps(spec)} isPresented onDismiss={onClose} testID={testID}>
      {/* iOS sheets rest at a fixed detent, so the plate takes the sheet's size; Android's
          sheet sizes to its content, so the plate takes the sheet's width and its own height. */}
      <RNHostView matchContents={Platform.OS === "ios" ? false : { vertical: true }}>
        <Faceplate style={{ flex: 0, flexGrow: 1 }}>
          {/* The native sheet already keeps its content clear of the home indicator. */}
          <View
            testID={`${testID}-body`}
            style={{ paddingHorizontal: 16, paddingTop: 8, paddingBottom: 16, gap: 12 }}
          >
            <View
              style={{
                flexDirection: "row",
                alignItems: "center",
                justifyContent: "space-between",
                minHeight: 36,
              }}
            >
              <Text
                accessibilityRole="header"
                maxFontSizeMultiplier={1.3}
                testID={`${testID}-caption`}
                style={[type.label, { color: colors.tertiaryLabel }]}
              >
                {caption}
              </Text>
              {done ? (
                <PanelKey
                  label="Done"
                  onPress={onClose}
                  testID={`${testID}-done`}
                  style={{ minHeight: 36, paddingHorizontal: 12 }}
                />
              ) : null}
            </View>
            {children}
          </View>
        </Faceplate>
      </RNHostView>
    </BottomSheet>
  );
}
