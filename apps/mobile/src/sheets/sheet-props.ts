import type { BottomSheetProps } from "@expo/ui";
import { environment, presentationBackground, tint } from "@expo/ui/swift-ui/modifiers";
import { Platform } from "react-native";
import { colors } from "@/theme";

export type SheetSpec =
  | { kind: "source" }
  | {
      kind: "output";
      count: number /** A latency-trim editor is shown below the rows. */;
      trim?: boolean;
      /** The playing-together area: membership rows and group cards. */
      groups?: { speakers: number; groups: number };
    }
  | { kind: "device-pair"; transport: string };

type SheetProps = Pick<BottomSheetProps, "modifiers" | "snapPoints" | "showDragIndicator">;

function iosSnapPoints(spec: SheetSpec): NonNullable<BottomSheetProps["snapPoints"]> {
  switch (spec.kind) {
    case "source":
      return [{ height: 280 }];
    case "output": {
      // Rows carry a sync line; the trim editor and the playing-together
      // area add their own blocks below them.
      const groups = spec.groups ? 190 + spec.groups.speakers * 52 + spec.groups.groups * 130 : 0;
      const height = 160 + spec.count * 96 + (spec.trim ? 150 : 0) + groups;
      // Taller than a phone screen: snap full instead of clipping the tail.
      return spec.count > 5 || height > 800 ? ["full"] : [{ height: Math.max(300, height) }];
    }
    case "device-pair":
      return [{ height: spec.transport === "airplay" ? 360 : 300 }];
  }
}

/**
 * Presentation for the mixer's bottom sheets. iOS gets compact detents and the
 * dark presentation explicitly, because BottomSheet creates its own native
 * host that the app host's color scheme does not cross. Android uses the
 * Material defaults, which already follow the app's dark appearance.
 */
export function sheetProps(spec: SheetSpec): SheetProps {
  return (
    Platform.select<SheetProps>({
      ios: {
        showDragIndicator: true,
        modifiers: [
          environment({ key: "colorScheme", value: "dark" }),
          presentationBackground(colors.surface),
          tint(colors.accent),
        ],
        snapPoints: iosSnapPoints(spec),
      },
      default: { showDragIndicator: true },
    }) ?? { showDragIndicator: true }
  );
}
