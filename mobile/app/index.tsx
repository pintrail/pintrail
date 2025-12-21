import React, { useState } from 'react';
import { View } from 'react-native';
import MapView, { Region } from 'react-native-maps';

export default function HomeScreen() {
  const [region, setRegion] = useState<Region>({
    latitude: 42.391702,
    longitude: -72.527101,
    latitudeDelta: 0.01,
    longitudeDelta: 0.01,
  });

  return (
    <View className="flex-1">
      <MapView
        style={{ flex: 1 }}
        region={region}
        onRegionChange={setRegion}
        showsUserLocation
        showsMyLocationButton
      />
    </View>
  );
}
