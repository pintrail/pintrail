import { Stack } from 'expo-router';
import 'react-native-reanimated';

export default function RootLayout() {
  return (
    <Stack
      screenOptions={{
        headerStyle: { backgroundColor: '#221e1a' },
        headerTintColor: '#c97d3a',
        headerTitleStyle: { color: '#e8e0d8', fontWeight: '600' },
        contentStyle: { backgroundColor: '#1a1714' },
      }}
    >
      <Stack.Screen name="(tabs)" options={{ headerShown: false, title: 'Back' }} />
      <Stack.Screen name="[id]" options={{ title: '' }} />
    </Stack>
  );
}
