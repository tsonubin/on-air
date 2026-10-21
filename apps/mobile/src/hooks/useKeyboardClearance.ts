import { type RefObject, useCallback, useEffect, useRef } from "react";
import {
  Keyboard,
  type KeyboardEvent,
  type NativeScrollEvent,
  type NativeSyntheticEvent,
  Platform,
  type ScrollView,
  type TextInput,
  type View,
} from "react-native";

/** Room kept between the cleared view and the top of the keyboard. */
const CLEARANCE = 12;

/**
 * Keeps a view that sits under a text field above the iOS keyboard while that field has focus.
 *
 * `automaticallyAdjustKeyboardInsets` only scrolls the focused field itself into view, so a
 * button under it stays behind the keyboard. On iOS 26 and later that is easy to miss: the
 * accessory bar React Native adds to number pads (the one with "Done") is see-through, yet it
 * still takes every tap in its frame, so the button shows through it and does not respond.
 *
 * Attach `scrollRef` and `onScroll` to the ScrollView, and `targetRef` and `onTargetLayout` to
 * the view to keep clear.
 */
export function useKeyboardClearance(fieldRef: RefObject<TextInput | null>) {
  const scrollRef = useRef<ScrollView>(null);
  const targetRef = useRef<View>(null);
  const scrollY = useRef(0);

  const clear = useCallback(
    (keyboard: KeyboardEvent["endCoordinates"] | undefined) => {
      // Android pads the whole screen for the keyboard instead (see PairingHome). A frame with no
      // height covers nothing; React Native's ScrollView reads it as no keyboard too.
      if (Platform.OS !== "ios" || !keyboard?.height || !fieldRef.current?.isFocused()) return;
      targetRef.current?.measureInWindow((_x, y, _width, height) => {
        const overlap = y + height + CLEARANCE - keyboard.screenY;
        if (overlap > 0) {
          scrollRef.current?.scrollTo({ y: scrollY.current + overlap, animated: true });
        }
      });
    },
    [fieldRef],
  );

  useEffect(() => {
    // "Did" rather than "will": by then the ScrollView has inset itself and scrolled the field
    // into view, so the measurement and the scroll offset are final.
    const subscription = Keyboard.addListener("keyboardDidShow", (event) =>
      clear(event.endCoordinates),
    );
    return () => subscription.remove();
  }, [clear]);

  const onScroll = useCallback((event: NativeSyntheticEvent<NativeScrollEvent>) => {
    scrollY.current = event.nativeEvent.contentOffset.y;
  }, []);

  // The view can also move while the keyboard stays up, such as when a card opens above it.
  const onTargetLayout = useCallback(() => clear(Keyboard.metrics()), [clear]);

  return { scrollRef, targetRef, onScroll, onTargetLayout };
}
