import { BottomSheet, RNHostView } from "@expo/ui";
import type { ReactNode } from "react";
import { Platform, Text, useWindowDimensions, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { colors, type } from "@/theme";
import { Faceplate } from "@/ui/faceplate";
import { PanelKey } from "@/ui/panel-key";
import { type SheetSpec, sheetProps } from "./sheet-props";

/**
 * A bottom sheet cut from the same faceplate as the mixer: the native sheet
 * presents and dismisses it, and the faceplate fills it edge to edge under a
 * silkscreened caption. On Android a Done key sits beside the caption (unless the
 * sheet has its own Cancel); iOS closes the sheet by swipe or a tap outside.
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
  const insets = useSafeAreaInsets();
  const { height } = useWindowDimensions();
  const ios = Platform.OS === "ios";
  return (
    <BottomSheet {...sheetProps(spec)} isPresented onDismiss={onClose} testID={testID}>
      {/* iOS sheets rest at a fixed detent, so the plate takes the sheet's size. Android's sheet
          sizes to its content, so the plate takes the sheet's width and its own height, capped
          below the system bars and the 56 pt drag-handle zone so a long list scrolls. */}
      <RNHostView matchContents={ios ? false : { vertical: true }}>
        <Faceplate
          testID={`${testID}-plate`}
          style={{
            flex: 0,
            flexGrow: 1,
            maxHeight: ios ? undefined : height - insets.top - insets.bottom - 56,
          }}
        >
          {/* The native sheet keeps its content clear of the home indicator; the plate runs
              under a side rail, so the content pads itself clear of one. */}
          <View
            testID={`${testID}-body`}
            style={{
              flex: 1,
              paddingLeft: Math.max(insets.left, 16),
              paddingRight: Math.max(insets.right, 16),
              paddingTop: 8,
              paddingBottom: 16,
              gap: 12,
            }}
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
              {/* iOS closes by swipe or a tap outside, so nothing sits beside the folded
                  iPhone Duo's rail, which holds the system's own controls. */}
              {done && !ios ? (
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
