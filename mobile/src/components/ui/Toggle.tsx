import { Switch, View, Text } from 'react-native';

interface ToggleProps {
  value: boolean;
  onValueChange: (value: boolean) => void;
  label?: string;
  className?: string;
}

export function Toggle({ value, onValueChange, label, className }: ToggleProps) {
  return (
    <View className={`flex-row items-center justify-between ${className || ''}`}>
      {label && (
        <Text className="text-base text-gray-700 mr-3">{label}</Text>
      )}
      <Switch
        value={value}
        onValueChange={onValueChange}
        trackColor={{ false: '#d1d5db', true: '#3b82f6' }}
        thumbColor={value ? '#ffffff' : '#f3f4f6'}
      />
    </View>
  );
}

