// Fonts are ready by default; a test overrides one call to see loading or a failure.
module.exports = {
  useFonts: jest.fn(() => [true, null]),
};
