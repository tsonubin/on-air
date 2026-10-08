const React = require("react");

function mock(name) {
  return function Mock(props) {
    return React.createElement(name, props, props.children);
  };
}

const Icon = mock("ExpoIcon");
Icon.select = ({ ios }) => ios;

// Interactive mocks drop their handlers when disabled, like the native views do.
module.exports = {
  BottomSheet: mock("ExpoBottomSheet"),
  Button: function Button(props) {
    return React.createElement(
      "ExpoButton",
      { ...props, onPress: props.disabled ? undefined : props.onPress },
      props.label ?? props.children,
    );
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
  Icon,
  ListItem: Object.assign(mock("ExpoListItem"), {
    Trailing: mock("ExpoListTrailing"),
  }),
  Picker: Object.assign(
    function Picker(props) {
      return React.createElement(
        "ExpoPicker",
        { ...props, onValueChange: props.enabled === false ? undefined : props.onValueChange },
        props.children,
      );
    },
    { Item: mock("ExpoPickerItem") },
  ),
  RNHostView: mock("ExpoRNHostView"),
  Row: mock("ExpoRow"),
  Slider: function Slider(props) {
    return React.createElement("ExpoSlider", {
      ...props,
      onValueChange: props.disabled ? undefined : props.onValueChange,
    });
  },
  Spacer: mock("ExpoSpacer"),
  ScrollView: mock("ExpoScrollView"),
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
