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
function Stack({ children }) {
  const route = React.useSyncExternalStore(subscribe, snapshot);
  const protectedGroup = React.Children.toArray(children).find((c) => c.type === Stack.Protected);
  const allowed = protectedGroup?.props.guard;
  const selected = allowed ? route : "/";
  const screens = {
    "/": () => require("../app/index").default,
    "/sound": () => require("../app/sound").default,
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
    selected !== "/" &&
      React.createElement("NativeBackButton", { testID: "native-back", onPress: router.back }),
    React.createElement(screens[selected]()),
  );
}
Stack.Screen = () => null;
Stack.Protected = () => null;
Stack.Toolbar = (props) => React.createElement("NativeToolbar", props, props.children);
Stack.Toolbar.Button = (props) => React.createElement("NativeToolbarButton", props, props.children);
module.exports = {
  router,
  Stack,
  __reset: () => {
    history = ["/"];
  },
};
