import { Text as RNText, TextProps } from 'react-native';

interface CustomTextProps extends TextProps {
  className?: string;
}

export function Text({ className, ...props }: CustomTextProps) {
  return <RNText className={className} {...props} />;
}
