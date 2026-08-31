const React = require("react");

function mock(name) {
  return function Mock(props) {
    return React.createElement(name, props, props.children);
  };
}

module.exports = {
  ActivityIndicator: mock("ActivityIndicator"),
  Pressable: mock("Pressable"),
  SafeAreaView: mock("SafeAreaView"),
  ScrollView: mock("ScrollView"),
  StyleSheet: { create: (s) => s },
  Text: mock("Text"),
  TextInput: mock("TextInput"),
  View: mock("View"),
  Platform: { OS: "ios" },
};
