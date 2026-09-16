// Native modifiers are serialized configuration; the renderer consumes these on device.
const modifier = ($type, params) => ({ $type, ...params });
module.exports = {
  environment: (config) => modifier("environment", config),
  fixedSize: (config) => modifier("fixedSize", config),
  frame: (config) => modifier("frame", config),
  layoutPriority: (priority) => modifier("layoutPriority", { priority }),
  presentationBackground: (color) => modifier("presentationBackground", { color }),
  tint: (color) => modifier("tint", { color }),
};
