// Native modifiers are serialized configuration; the renderer consumes these on device.
const modifier = ($type, params) => ({ $type, ...params });
module.exports = {
  accessibilityLabel: (label) => modifier("accessibilityLabel", { label }),
  background: (color) => modifier("background", { color }),
  environment: (config) => modifier("environment", config),
  font: (params) => modifier("font", params),
  listRowBackground: (color) => modifier("listRowBackground", { color }),
  scrollContentBackground: (visible) => modifier("scrollContentBackground", { visible }),
  fixedSize: (config) => modifier("fixedSize", config),
  frame: (config) => modifier("frame", config),
  ignoreSafeArea: (params) => modifier("ignoreSafeArea", params ?? {}),
  layoutPriority: (priority) => modifier("layoutPriority", { priority }),
  presentationBackground: (color) => modifier("presentationBackground", { color }),
  textSelection: (value) => modifier("textSelection", { value }),
  tint: (color) => modifier("tint", { color }),
};
