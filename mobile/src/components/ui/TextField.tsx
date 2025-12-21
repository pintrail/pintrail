import { TextInput, TextInputProps, View, Text } from 'react-native';

interface TextFieldProps extends TextInputProps {
  label?: string;
  error?: string;
  className?: string;
  inputClassName?: string;
}

export function TextField({
  label,
  error,
  className,
  inputClassName,
  ...props
}: TextFieldProps) {
  return (
    <View className={className}>
      {label && (
        <Text className="text-sm font-medium text-gray-700 mb-1">{label}</Text>
      )}
      <TextInput
        className={`border border-gray-300 rounded-lg px-3 py-2 text-base ${inputClassName || ''}`}
        placeholderTextColor="#9ca3af"
        {...props}
      />
      {error && (
        <Text className="text-sm text-red-600 mt-1">{error}</Text>
      )}
    </View>
  );
}

