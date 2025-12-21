import React, { useState } from 'react';
import MapView, { Region } from 'react-native-maps';
import { StyleSheet, View } from 'react-native';

export default function HomeScreen() {

  const getInitialRegion = () => {
    return {
      latitude: 42.391702,
      longitude: -72.527101,
      latitudeDelta: 0.01,
      longitudeDelta: 0.01,
    }
  };

  const onRegionChange = (region: Region) => {
    setState(region);
  };

  const [state, setState] = useState(getInitialRegion());

  return (
    <View style={styles.container}>
      <MapView style={styles.map} region={state} onRegionChange={onRegionChange} />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  map: {
    width: '100%',
    height: '100%',
  },
});

