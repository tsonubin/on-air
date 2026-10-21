const React = require("react");
module.exports = {
  DarkTheme: {
    dark: true,
    colors: {
      primary: "#0a84ff",
      background: "#010101",
      card: "#121212",
      text: "#e5e5e7",
      border: "#272729",
      notification: "#ff453a",
    },
    fonts: {},
  },
  ThemeProvider: ({ children }) => React.createElement(React.Fragment, {}, children),
};
