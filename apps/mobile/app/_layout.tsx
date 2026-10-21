import { ThemeProvider } from "expo-router";
import { Stack } from "expo-router/stack";
import { StatusBar } from "expo-status-bar";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { FontGate } from "@/font-gate";
import { navigationTheme, screenHeaderOptions, stackScreens } from "@/navigation-theme";
import { RemoteProvider, useRemoteSession } from "@/remote-session";

function Navigation() {
  const { paired } = useRemoteSession();
  return (
    <Stack screenOptions={{ headerBackButtonDisplayMode: "default", ...screenHeaderOptions }}>
      <Stack.Screen name="index" options={{ headerShown: false, title: "Mixer" }} />
      <Stack.Protected guard={paired}>
        {stackScreens.map((screen) => (
          <Stack.Screen key={screen.name} name={screen.name} options={screen.options} />
        ))}
      </Stack.Protected>
    </Stack>
  );
}

// The app is dark-only (`userInterfaceStyle: "dark"` in app.json). Expo Router's
// Stack reads its own theme context, so ThemeProvider comes from expo-router and
// the faceplate theme builds on its DarkTheme; otherwise the native headers fall
// back to the light theme.
export default function RootLayout() {
  return (
    <SafeAreaProvider>
      <FontGate>
        <ThemeProvider value={navigationTheme}>
          <StatusBar style="light" />
          <RemoteProvider>
            <Navigation />
          </RemoteProvider>
        </ThemeProvider>
      </FontGate>
    </SafeAreaProvider>
  );
}
