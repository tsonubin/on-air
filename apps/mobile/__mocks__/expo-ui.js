const React = require("react");

function mock(name) {
  return function Mock(props) {
    return React.createElement(name, props, props.children);
  };
}

// FieldGroup.Section slots, extracted like Expo UI's FieldSectionSlots: only a
// direct SectionHeader / SectionFooter element (or one inside a Fragment) fills a
// slot; anything else, including a component that renders one, becomes a row.
function SectionHeader(props) {
  return React.createElement(React.Fragment, {}, props.children);
}
function SectionFooter(props) {
  return React.createElement(React.Fragment, {}, props.children);
}
function FieldSection(props) {
  let header;
  let footer;
  const rows = [];
  const walk = (node) => {
    React.Children.forEach(node, (child) => {
      if (child == null || typeof child === "boolean") return;
      if (!React.isValidElement(child)) return rows.push(child);
      if (child.type === SectionHeader) header = child.props.children;
      else if (child.type === SectionFooter) footer = child.props.children;
      else if (child.type === React.Fragment) walk(child.props.children);
      else rows.push(child);
    });
  };
  walk(props.children);
  return React.createElement(
    "ExpoFieldSection",
    { ...props, children: undefined },
    header === undefined ? null : React.createElement("ExpoFieldSectionHeader", {}, header),
    React.createElement("ExpoFieldSectionRows", {}, ...rows),
    footer === undefined ? null : React.createElement("ExpoFieldSectionFooter", {}, footer),
  );
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
    Section: FieldSection,
    SectionHeader: SectionHeader,
    SectionFooter: SectionFooter,
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
