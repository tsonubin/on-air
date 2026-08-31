const React = require("react");

function mock(name) {
  return function Mock(props) {
    return React.createElement(name, props, props.children);
  };
}

module.exports = {
  BottomSheet: mock("ExpoBottomSheet"),
  Button: function Button(props) {
    return React.createElement("ExpoButton", props, props.label ?? props.children);
  },
  Collapsible: function Collapsible(props) {
    return React.createElement("ExpoCollapsible", props, props.isOpen ? props.children : null);
  },
  Column: mock("ExpoColumn"),
  FieldGroup: Object.assign(mock("ExpoFieldGroup"), {
    Section: mock("ExpoFieldSection"),
    SectionHeader: mock("ExpoFieldSectionHeader"),
    SectionFooter: mock("ExpoFieldSectionFooter"),
  }),
  Host: mock("ExpoHost"),
  Picker: Object.assign(mock("ExpoPicker"), {
    Item: mock("ExpoPickerItem"),
  }),
  RNHostView: mock("ExpoRNHostView"),
  Row: mock("ExpoRow"),
  Slider: mock("ExpoSlider"),
  Spacer: mock("ExpoSpacer"),
  Text: mock("ExpoText"),
  TextInput: function TextInput(props) {
    return React.createElement("ExpoTextInput", {
      ...props,
      value: props.value?.value ?? props.defaultValue ?? "",
    });
  },
  useNativeState: function useNativeState(initialValue) {
    return React.useRef({ value: initialValue }).current;
  },
};
