module.exports = {
  testEnvironment: "node",
  testMatch: ["<rootDir>/__tests__/**/*.test.{ts,tsx}"],
  setupFiles: ["<rootDir>/jest.setup.js"],
  transform: {
    "^.+\\.(js|jsx|ts|tsx)$": ["babel-jest", { configFile: "./babel.config.js" }],
  },
  transformIgnorePatterns: [
    "/node_modules/(?!(.pnpm/)?(@on-air|expo|@expo|react-native|@react-native))",
  ],
  moduleNameMapper: {
    "^@/(.*)$": "<rootDir>/src/$1",
    "^expo-router(/stack)?$": "<rootDir>/__mocks__/expo-router.js",
    "^@react-navigation/native$": "<rootDir>/__mocks__/navigation-theme.js",
    "^@on-air/control-client$": "<rootDir>/../../packages/control-client/src/index.ts",
    "^@on-air/api-types$": "<rootDir>/../../packages/api-types/src/index.ts",
    "^react-native$": "<rootDir>/__mocks__/react-native.js",
    "^expo-network$": "<rootDir>/__mocks__/expo-network.js",
    "^expo-secure-store$": "<rootDir>/__mocks__/expo-secure-store.js",
    "^@expo/ui/swift-ui/modifiers$": "<rootDir>/__mocks__/swift-ui-modifiers.js",
    "^@expo/ui$": "<rootDir>/__mocks__/expo-ui.js",
    "^@expo/material-symbols/.*\\.xml$": "<rootDir>/__mocks__/material-symbol.js",
    "^expo-status-bar$": "<rootDir>/__mocks__/expo-status-bar.js",
    "^react-native-safe-area-context$": "<rootDir>/__mocks__/safe-area-context.js",
  },
};
