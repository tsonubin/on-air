const React = require("react");
let history = ["/"];
const listeners = new Set();
const notify = () => {
  for (const fn of listeners) fn();
};
const router = {
  push: (path) => {
    history.push(path);
    notify();
  },
  back: () => {
    if (history.length > 1) history.pop();
    notify();
  },
  dismissAll: () => {
    history = ["/"];
    notify();
  },
  replace: (path) => {
    history[history.length - 1] = path;
    notify();
  },
};
const snapshot = () => history[history.length - 1];
const subscribe = (fn) => {
  listeners.add(fn);
  return () => listeners.delete(fn);
};

// Header options, as the native stack keeps them. A page's <Stack.Screen options> calls
// setOptions and never undoes it, so its options stay on the route after the page's
// content unmounts (until the route itself goes). A left/right <Stack.Toolbar> shows the
// header only while it is mounted. Kept per route path, cleared by __reset.
let routeOptions = {};
let routeToolbars = {};
let headerVersion = 0;
const headerListeners = new Set();
const headerChanged = () => {
  headerVersion += 1;
  for (const fn of headerListeners) fn();
};
const subscribeHeader = (fn) => {
  headerListeners.add(fn);
  return () => headerListeners.delete(fn);
};
const RouteContext = React.createContext("/");

/** The options a layout's <Stack.Screen name> children give each route path. */
function layoutOptions(children) {
  const byPath = {};
  const walk = (nodes) =>
    React.Children.forEach(nodes, (child) => {
      if (!React.isValidElement(child)) return;
      if (child.type === Stack.Protected) return walk(child.props.children);
      if (child.type === Stack.Screen && child.props.name) {
        const path = child.props.name === "index" ? "/" : `/${child.props.name}`;
        byPath[path] = child.props.options ?? {};
      }
    });
  walk(children);
  return byPath;
}

function Stack({ children }) {
  const route = React.useSyncExternalStore(subscribe, snapshot);
  React.useSyncExternalStore(subscribeHeader, () => headerVersion);
  const protectedGroup = React.Children.toArray(children).find((c) => c.type === Stack.Protected);
  const allowed = protectedGroup?.props.guard;
  const selected = allowed ? route : "/";
  const header = {
    ...layoutOptions(children)[selected],
    ...routeOptions[selected],
    ...(routeToolbars[selected] > 0 ? { headerShown: true } : {}),
  };
  const screens = {
    "/": () => require("../app/index").default,
    "/equalizer": () => require("../app/equalizer").default,
    "/audio-format": () => require("../app/audio-format").default,
    "/connection": () => require("../app/connection").default,
  };
  React.useEffect(() => {
    if (!allowed && route !== "/") router.dismissAll();
  }, [allowed, route]);
  return React.createElement(
    "NativeStack",
    {},
    // The route's effective header options, as a function so they stay out of snapshots.
    React.createElement("NativeHeader", { route: selected, headerOptions: () => header }),
    selected !== "/" &&
      React.createElement("NativeBackButton", { testID: "native-back", onPress: router.back }),
    React.createElement(
      RouteContext.Provider,
      { value: selected },
      React.createElement(screens[selected]()),
    ),
  );
}
// A screen's own <Stack.Screen options> sets its route's options (kept after it unmounts,
// like setOptions) and renders its header buttons so tests can press them.
Stack.Screen = function Screen({ options }) {
  const route = React.useContext(RouteContext);
  React.useLayoutEffect(() => {
    if (!options || Object.keys(options).length === 0) return;
    const next = { ...routeOptions[route], ...options };
    // Functions (headerRight) are rebuilt every render; compare the rest.
    if (JSON.stringify(next) === JSON.stringify(routeOptions[route] ?? {})) return;
    routeOptions = { ...routeOptions, [route]: next };
    headerChanged();
  });
  return typeof options?.headerRight === "function"
    ? React.createElement("NativeHeaderRight", {}, options.headerRight({ canGoBack: true }))
    : null;
};
Stack.Protected = () => null;
// A left/right toolbar shows its route's header while it is mounted, as expo-router does.
Stack.Toolbar = function Toolbar(props) {
  const route = React.useContext(RouteContext);
  const inHeader = props.placement === "left" || props.placement === "right";
  React.useLayoutEffect(() => {
    if (!inHeader) return;
    routeToolbars = { ...routeToolbars, [route]: (routeToolbars[route] ?? 0) + 1 };
    headerChanged();
    return () => {
      routeToolbars = { ...routeToolbars, [route]: routeToolbars[route] - 1 };
      headerChanged();
    };
  }, [route, inHeader]);
  return React.createElement("NativeToolbar", props, props.children);
};
Stack.Toolbar.Button = (props) => React.createElement("NativeToolbarButton", props, props.children);
module.exports = {
  DarkTheme: {
    dark: true,
    colors: {
      primary: "#0a84ff",
      background: "#010101",
      card: "#121212",
      text: "#e5e5e7",
      border: "#272729",
      notification: "#ff453a",
    },
    fonts: {},
  },
  ThemeProvider: ({ children }) => React.createElement(React.Fragment, {}, children),
  router,
  Stack,
  __reset: () => {
    history = ["/"];
    routeOptions = {};
    routeToolbars = {};
  },
};
