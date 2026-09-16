const React = require("react");
module.exports = {
  DarkTheme: { dark: true },
  DefaultTheme: { dark: false },
  ThemeProvider: ({ children }) => React.createElement(React.Fragment, {}, children),
};
