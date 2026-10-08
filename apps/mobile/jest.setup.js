global.IS_REACT_ACT_ENVIRONMENT = true;

// RNTL 13 renders through react-test-renderer, which logs a deprecation notice
// on every create(). The notice is not actionable here; keep the output useful.
const consoleError = console.error.bind(console);
console.error = (...args) => {
  if (typeof args[0] === "string" && args[0].startsWith("react-test-renderer is deprecated")) {
    return;
  }
  consoleError(...args);
};
