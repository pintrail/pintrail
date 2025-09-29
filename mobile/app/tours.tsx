import React from 'react';
import { View, Text, StyleSheet } from 'react-native';

export default function ToursView() {
  return (
    <View style={styles.container}>
      <Text style={styles.text}>ToursView</Text>
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
