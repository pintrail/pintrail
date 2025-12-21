import { View } from 'react-native';
import { Text, Button } from '../src/components';

export default function DetailScreen() {
  const handlePress = () => {
    console.log('Button pressed!');
  };

  return (
    <View className="flex-1 items-center justify-center p-4 bg-white">
      <Text className="text-2xl font-bold mb-4">Detail Screen</Text>
      
      <Button
        title="Click Me"
        onPress={handlePress}
        className="mb-4"
      />
    </View>
  );
}
