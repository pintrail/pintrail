import { TouchableOpacity, Text } from 'react-native';

interface ButtonProps {
  title: string;
  onPress: () => void;
  className?: string;
}

export function Button({ title, onPress, className }: ButtonProps) {
  return (
    <TouchableOpacity
      onPress={onPress}
      className={`bg-blue-600 px-4 py-3 rounded-lg items-center ${className || ''}`}
    >
      <Text className="text-white font-semibold">{title}</Text>
    </TouchableOpacity>
  );
}
