import type { BottomSheetProps } from "@expo/ui";
import { environment, tint } from "@expo/ui/swift-ui/modifiers";
import { Platform } from "react-native";
import { colors } from "@/theme";

export type SheetSpec =
  | { kind: "source"; count: number }
  | { kind: "output"; count: number }
  | { kind: "device-pair"; transport: string };

type SheetProps = Pick<
  BottomSheetProps,
  "modifiers" | "snapPoints" | "showDragIndicator" | "containerColor" | "contentPadding"
>;

/** The channel strips sit 66 pt apart; above five the sheet opens full and scrolls. */
const STRIP = 66;

const android: SheetProps = {
  showDragIndicator: true,
  containerColor: colors.faceTop,
  contentPadding: 0,
};

function iosSnapPoints(spec: SheetSpec): NonNullable<BottomSheetProps["snapPoints"]> {
  switch (spec.kind) {
    case "source":
      return spec.count > 5 ? ["full"] : [{ height: Math.max(280, 160 + spec.count * STRIP) }];
    case "output":
      return spec.count > 5 ? ["full"] : [{ height: Math.max(300, 200 + spec.count * STRIP) }];
    case "device-pair":
      return [{ height: spec.transport === "airplay" ? 360 : 300 }];
  }
}

/**
 * Presentation for the mixer's bottom sheets. Their React Native content draws
 * the faceplate edge to edge, so the sheet itself is painted the faceplate's
 * top color with no inset. BottomSheet creates its own native host that the
 * app's color scheme and seed do not cross, so iOS also gets compact detents,
 * the dark scheme (for the drag indicator) and the amber tint explicitly.
 */
export function sheetProps(spec: SheetSpec): SheetProps {
  return (
    Platform.select<SheetProps>({
      ios: {
        showDragIndicator: true,
        // iOS keeps the content above the home indicator; that strip continues the plate's lower edge.
        containerColor: colors.faceBottom,
        contentPadding: 0,
        modifiers: [environment({ key: "colorScheme", value: "dark" }), tint(colors.amber)],
        snapPoints: iosSnapPoints(spec),
      },
      default: android,
    }) ?? android
  );
}
