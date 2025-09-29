import React from 'react';
import { View, Text, StyleSheet } from 'react-native';

export default function SearchView() {
  return (
    <View style={styles.container}>
      <Text style={styles.text}>SearchView</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#f8fafc',
  },
  text: {
    fontSize: 24,
    fontWeight: 'bold',
    color: '#1e293b',
  },
});
