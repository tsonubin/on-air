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
 * Keeps a view that sits under a text field above the keyboard while that field has focus.
 *
 * Both platforms only scroll the focused field itself into view, so a button under it stays
 * covered. iOS insets the ScrollView (`automaticallyAdjustKeyboardInsets`) and scrolls to the
 * field. On iOS 26 and later the covered button is easy to miss: the accessory bar React Native
 * adds to number pads (the one with "Done") is see-through, yet it still takes every tap in its
 * frame. Android pads the screen instead (see PairingHome), and its ScrollView stops once the
 * field's bottom edge meets the keyboard, which leaves the button clipped below it.
 *
 * Attach `scrollRef`, `onScroll` and `onLayout` to the ScrollView, `targetRef` and `onLayout` to
 * the view to keep clear, and `onFocus` to the field.
 */
export function useKeyboardClearance(fieldRef: RefObject<TextInput | null>) {
  const scrollRef = useRef<ScrollView>(null);
  const targetRef = useRef<View>(null);
  const scrollY = useRef(0);

  const clear = useCallback(
    (keyboard: KeyboardEvent["endCoordinates"] | undefined) => {
      // A frame with no height covers nothing; React Native's ScrollView reads it as no keyboard
      // too. The measurement and `screenY` share one space on both platforms: the window, which
      // edge-to-edge Android draws from the top of the screen.
      if (!keyboard?.height || !fieldRef.current?.isFocused()) return;
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
    // "Did" rather than "will": by then the iOS ScrollView has inset itself and scrolled the
    // field into view, so the measurement and the scroll offset are final. Android only has
    // "did", and it comes before the padding (see onLayout).
    const subscription = Keyboard.addListener("keyboardDidShow", (event) =>
      clear(event.endCoordinates),
    );
    return () => subscription.remove();
  }, [clear]);

  const onScroll = useCallback((event: NativeSyntheticEvent<NativeScrollEvent>) => {
    scrollY.current = event.nativeEvent.contentOffset.y;
  }, []);

  // Either view can change while the keyboard stays up. The target moves when a card opens above
  // it. On Android the ScrollView only shrinks for the keyboard after keyboardDidShow, once the
  // padding lands, so a scroll from that event alone stops short at the old scroll range.
  const onLayout = useCallback(() => clear(Keyboard.metrics()), [clear]);

  // iOS sends keyboardDidShow whenever a field takes focus. Android sends nothing when focus moves
  // to the field while the keyboard stays up at the same height, as from an address field's "next".
  const onFocus = useCallback(() => {
    if (Platform.OS === "android") clear(Keyboard.metrics());
  }, [clear]);

  return { scrollRef, targetRef, onScroll, onLayout, onFocus };
}
