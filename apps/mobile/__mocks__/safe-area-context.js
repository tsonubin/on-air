const React = require("react");

module.exports = {
  SafeAreaProvider: (props) => React.createElement("SafeAreaProvider", props, props.children),
  SafeAreaView: (props) => React.createElement("SafeAreaView", props, props.children),
  useSafeAreaInsets: () => ({ top: 47, right: 0, bottom: 34, left: 0 }),
};
