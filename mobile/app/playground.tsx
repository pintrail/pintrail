import { useState } from 'react';
import { View, ScrollView, FlatList } from 'react-native';
import { Text, Button, TextField, Toggle } from '../src/components';

const SAMPLE_DATA = [
  { id: '1', title: 'Item 1', description: 'First item' },
  { id: '2', title: 'Item 2', description: 'Second item' },
  { id: '3', title: 'Item 3', description: 'Third item' },
];

export default function PlaygroundScreen() {
  const [textValue, setTextValue] = useState('');
  const [toggleValue, setToggleValue] = useState(false);

  return (
    <ScrollView 
      className="flex-1 bg-gray-50 mt-10"
      contentContainerStyle={{ paddingBottom: 32 }}
    >
      <View className="p-4">
        <Text className="text-3xl font-bold mb-2">Component Playground</Text>
        <Text className="text-gray-600 mb-6">Explore React Native components</Text>

        {/* Text */}
        <View className="mb-6">
          <Text className="text-2xl font-bold mb-4">Text</Text>
          <View className="bg-white p-4 rounded-lg shadow-sm">
            <Text className="text-sm mb-2">Small</Text>
            <Text className="text-base mb-2">Base</Text>
            <Text className="text-lg mb-2">Large</Text>
            <Text className="text-xl font-bold mb-2">Bold XL</Text>
            <Text className="text-blue-600">Colored text</Text>
          </View>
        </View>

        {/* Buttons */}
        <View className="mb-6">
          <Text className="text-2xl font-bold mb-4">Buttons</Text>
          <View className="bg-white p-4 rounded-lg shadow-sm">
            <Button title="Primary" onPress={() => {}} className="mb-3" />
            <Button title="Colored" onPress={() => {}} className="mb-3 bg-blue-500" />
            <Button title="Disabled" onPress={() => {}} disabled className="opacity-50" />
          </View>
        </View>

        {/* Text Fields */}
        <View className="mb-6">
          <Text className="text-2xl font-bold mb-4">Text Fields</Text>
          <View className="bg-white p-4 rounded-lg shadow-sm">
            <TextField
              label="Default"
              placeholder="Enter text"
              value={textValue}
              onChangeText={setTextValue}
              className="mb-4"
            />
            <TextField
              label="Email"
              placeholder="email@example.com"
              keyboardType="email-address"
              className="mb-4"
            />
            <TextField
              label="With Error"
              placeholder="Error state"
              error="Required field"
              className="mb-4"
            />
          </View>
        </View>

        {/* Toggles */}
        <View className="mb-6">
          <Text className="text-2xl font-bold mb-4">Toggles</Text>
          <View className="bg-white p-4 rounded-lg shadow-sm">
            <Toggle
              label="Default Toggle"
              value={toggleValue}
              onValueChange={setToggleValue}
              className="mb-4"
            />
            <Toggle label="Enabled" value={true} onValueChange={() => {}} />
          </View>
        </View>

        {/* FlatList */}
        <View className="mb-6">
          <Text className="text-2xl font-bold mb-4">FlatList</Text>
          <View className="bg-white rounded-lg shadow-sm overflow-hidden">
            <FlatList
              data={SAMPLE_DATA}
              keyExtractor={(item) => item.id}
              scrollEnabled={false}
              renderItem={({ item }) => (
                <View className="px-4 py-3 border-b border-gray-200">
                  <Text className="text-base font-semibold mb-1">{item.title}</Text>
                  <Text className="text-sm text-gray-600">{item.description}</Text>
                </View>
              )}
            />
          </View>
        </View>

        {/* Cards */}
        <View className="mb-6">
          <Text className="text-2xl font-bold mb-4">Cards</Text>
          <View className="bg-white p-4 rounded-lg shadow-sm mb-4">
            <Text className="text-lg font-semibold mb-2">Default Card</Text>
            <Text className="text-gray-600">Card with shadow</Text>
          </View>
          <View className="bg-blue-500 p-4 rounded-lg">
            <Text className="text-lg font-semibold text-white mb-2">Colored Card</Text>
            <Text className="text-blue-100">Card with background</Text>
          </View>
        </View>
      </View>
    </ScrollView>
  );
}

