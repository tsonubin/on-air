module.exports = {
  testEnvironment: "node",
  forceExit: true,
  setupFiles: ["<rootDir>/jest.setup.js"],
  transform: {
    "^.+\\.(js|jsx|ts|tsx)$": ["babel-jest", { configFile: "./babel.config.js" }],
  },
  transformIgnorePatterns: [
    "/node_modules/(?!(.pnpm/)?(@on-air|expo|@expo|react-native|@react-native))",
  ],
  moduleNameMapper: {
    "^@on-air/control-client$": "<rootDir>/../../packages/control-client/src/index.ts",
    "^@on-air/api-types$": "<rootDir>/../../packages/api-types/src/index.ts",
    "^react-native$": "<rootDir>/__mocks__/react-native.js",
    "^expo-network$": "<rootDir>/__mocks__/expo-network.js",
    "^expo$": "<rootDir>/__mocks__/expo.js",
  },
};
