import type { BottomSheetProps } from "@expo/ui";
import { environment, presentationBackground, tint } from "@expo/ui/swift-ui/modifiers";
import { Platform } from "react-native";
import { colors } from "@/theme";

export type SheetSpec =
  | { kind: "source" }
  | { kind: "output"; count: number }
  | { kind: "device-pair"; transport: string };

type SheetProps = Pick<
  BottomSheetProps,
  "modifiers" | "snapPoints" | "showDragIndicator" | "containerColor" | "contentColor"
>;

const android: SheetProps = {
  showDragIndicator: true,
  containerColor: colors.surface,
  contentColor: colors.label,
};

function iosSnapPoints(spec: SheetSpec): NonNullable<BottomSheetProps["snapPoints"]> {
  switch (spec.kind) {
    case "source":
      return [{ height: 280 }];
    case "output":
      return spec.count > 5 ? ["full"] : [{ height: Math.max(300, 160 + spec.count * 78) }];
    case "device-pair":
      return [{ height: spec.transport === "airplay" ? 360 : 300 }];
  }
}

/**
 * Presentation for the mixer's bottom sheets. BottomSheet creates its own
 * native host that the app host's color scheme and seed do not cross, so iOS
 * gets compact detents, the dark presentation and the amber tint explicitly,
 * and Android gets the faceplate container with cream content.
 */
export function sheetProps(spec: SheetSpec): SheetProps {
  return (
    Platform.select<SheetProps>({
      ios: {
        showDragIndicator: true,
        modifiers: [
          environment({ key: "colorScheme", value: "dark" }),
          presentationBackground(colors.surface),
          tint(colors.amber),
        ],
        snapPoints: iosSnapPoints(spec),
      },
      default: android,
    }) ?? android
  );
}
