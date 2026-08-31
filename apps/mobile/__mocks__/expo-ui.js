const React = require("react");

function mock(name) {
  return function Mock(props) {
    return React.createElement(name, props, props.children);
  };
}

module.exports = {
  Button: function Button(props) {
    return React.createElement("ExpoButton", props, props.label ?? props.children);
  },
  Host: mock("ExpoHost"),
  Slider: mock("ExpoSlider"),
};
