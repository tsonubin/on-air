const React = require("react");
module.exports = {
  DarkTheme: { dark: true },
  ThemeProvider: ({ children }) => React.createElement(React.Fragment, {}, children),
};
