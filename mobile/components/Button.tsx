import { Text, TouchableOpacity } from 'react-native';

export default function Button() {
  return (
    <TouchableOpacity className="bg-blue-500 px-4 py-2 rounded-lg">
      <Text className="text-white font-bold text-center">Button</Text>
    </TouchableOpacity>
  );
}
