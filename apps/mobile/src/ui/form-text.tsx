import { FieldGroup, Text } from "@expo/ui";
import { font, textSelection } from "@expo/ui/swift-ui/modifiers";
import { Platform } from "react-native";
import { colors, fonts } from "@/theme";

type Variant = "body" | "footnote" | "label" | "value";

// SwiftUI text styles keep Dynamic Type scaling with the custom family.
const variants = {
  body: { family: fonts.sans, textStyle: "body", color: colors.label },
  footnote: { family: fonts.sans, textStyle: "footnote", color: colors.tertiaryLabel },
  label: { family: fonts.monoMedium, textStyle: "footnote", color: colors.tertiaryLabel },
  value: { family: fonts.monoMedium, textStyle: "body", color: colors.amber },
} as const;

/**
 * Text inside native settings forms. On iOS the vendored family is applied
 * through SwiftUI's font modifier; Android Compose text keeps the system
 * font and takes the color only. `label` is the uppercase mono caption.
 */
export function FormText({
  children,
  variant,
  copyable = false,
  testID,
}: {
  children: string;
  variant: Variant;
  /** Lets the person select and copy the text (iOS), e.g. the paired host address. */
  copyable?: boolean;
  testID?: string;
}) {
  const v = variants[variant];
  return (
    <Text
      testID={testID}
      textStyle={{ color: v.color }}
      modifiers={
        Platform.OS === "ios"
          ? [
              font({ family: v.family, textStyle: v.textStyle }),
              ...(copyable ? [textSelection(true)] : []),
            ]
          : undefined
      }
    >
      {variant === "label" ? children.toUpperCase() : children}
    </Text>
  );
}

/** A form section's title as an uppercase mono caption. */
export function SectionTitle({ title }: { title: string }) {
  return (
    <FieldGroup.SectionHeader>
      <FormText variant="label">{title}</FormText>
    </FieldGroup.SectionHeader>
  );
}
