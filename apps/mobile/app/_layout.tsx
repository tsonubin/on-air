import { ThemeProvider } from "@react-navigation/native";
import { Stack } from "expo-router/stack";
import { StatusBar } from "expo-status-bar";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { FontGate } from "@/font-gate";
import { navigationTheme, screenHeaderOptions } from "@/navigation-theme";
import { RemoteProvider, useRemoteSession } from "@/remote-session";

function Navigation() {
  const { paired } = useRemoteSession();
  return (
    <Stack screenOptions={{ headerBackButtonDisplayMode: "default", ...screenHeaderOptions }}>
      <Stack.Screen name="index" options={{ headerShown: false, title: "Mixer" }} />
      <Stack.Protected guard={paired}>
        <Stack.Screen name="sound" options={{ title: "Sound" }} />
        <Stack.Screen name="equalizer" options={{ title: "Equalizer" }} />
        <Stack.Screen name="audio-format" options={{ title: "Audio format" }} />
        <Stack.Screen name="connection" options={{ title: "Connection" }} />
      </Stack.Protected>
    </Stack>
  );
}

// The app is dark-only (`userInterfaceStyle: "dark"` in app.json).
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
