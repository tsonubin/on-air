const React = require("react");

function mock(name) {
  return function Mock(props) {
    return React.createElement(name, props, props.children);
  };
}

const Platform = {
  OS: "ios",
  select(spec) {
    if (Platform.OS in spec) return spec[Platform.OS];
    return spec.default;
  },
};

const appStateListeners = new Set();
const AppState = {
  currentState: "active",
  addEventListener(_type, listener) {
    appStateListeners.add(listener);
    return {
      remove() {
        appStateListeners.delete(listener);
      },
    };
  },
  __emit(next) {
    AppState.currentState = next;
    for (const listener of appStateListeners) listener(next);
  },
};

const keyboardListeners = new Map();
// Like the real one, `metrics()` reports the keyboard from the last `keyboardDidShow` until it hides.
let keyboardShowing = null;
const Keyboard = {
  metrics() {
    return keyboardShowing?.endCoordinates;
  },
  addListener(type, listener) {
    const listeners = keyboardListeners.get(type) ?? new Set();
    keyboardListeners.set(type, listeners);
    listeners.add(listener);
    return {
      remove() {
        listeners.delete(listener);
      },
    };
  },
  __emit(type, event) {
    if (type === "keyboardDidShow") keyboardShowing = event;
    if (type === "keyboardDidHide") keyboardShowing = null;
    for (const listener of keyboardListeners.get(type) ?? []) listener(event);
  },
};

// PanResponder maps the responder props a test fires to the config's pan
// callbacks, computing dx/dy from nativeEvent.pageX/pageY relative to the grant.
const PanResponder = {
  create(config) {
    let start = { x: 0, y: 0 };
    const gesture = (e) => ({
      dx: e.nativeEvent.pageX - start.x,
      dy: e.nativeEvent.pageY - start.y,
    });
    return {
      panHandlers: {
        onStartShouldSetResponder: (e) =>
          config.onStartShouldSetPanResponder?.(e, { dx: 0, dy: 0 }) ?? false,
        onResponderTerminationRequest: (e) =>
          config.onPanResponderTerminationRequest?.(e, gesture(e)) ?? true,
        onResponderGrant: (e) => {
          start = { x: e.nativeEvent.pageX, y: e.nativeEvent.pageY };
          config.onPanResponderGrant?.(e, { dx: 0, dy: 0 });
        },
        onResponderMove: (e) => config.onPanResponderMove?.(e, gesture(e)),
        onResponderRelease: (e) => config.onPanResponderRelease?.(e, gesture(e)),
        onResponderTerminate: (e) => config.onPanResponderTerminate?.(e, gesture(e)),
      },
    };
  },
};

// Pressable honours `disabled` and resolves function styles like the real one,
// so a test that presses a disabled control fails instead of passing silently.
function Pressable(props) {
  const style = typeof props.style === "function" ? props.style({ pressed: false }) : props.style;
  return React.createElement(
    "Pressable",
    { ...props, style, onPress: props.disabled ? undefined : props.onPress },
    props.children,
  );
}

module.exports = {
  ActivityIndicator: mock("ActivityIndicator"),
  AppState,
  Image: mock("Image"),
  Keyboard,
  KeyboardAvoidingView: mock("KeyboardAvoidingView"),
  PanResponder,
  Pressable,
  SafeAreaView: mock("SafeAreaView"),
  ScrollView: mock("ScrollView"),
  StyleSheet: { create: (s) => s, flatten: (s) => Object.assign({}, ...[s].flat(Infinity)) },
  Text: mock("Text"),
  TextInput: mock("TextInput"),
  View: mock("View"),
  Platform,
  useWindowDimensions: jest.fn(() => ({ width: 390, height: 844, scale: 3, fontScale: 1 })),
};
