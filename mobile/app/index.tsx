import React, { useState } from 'react';
import MapView, { Marker, Region } from 'react-native-maps';
import { StyleSheet, View, Text, Pressable } from 'react-native';

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

  type markerData = {
    coordinate: {longitude: number, latitude: number },
    id: number,
    title: string,
    pinColor: string,
    description: string,
    onPress: (e: any) => void
  }

  const markers: markerData[] = [
    {
      coordinate: { latitude: 42.39674910914364, longitude: -72.52415362726558 },
      id: 1,
      title: 'North B',
      pinColor: 'red',
      description: 'This is where Felix lives',
      onPress: (e) => { e.stopPropagation(); setOpen(1); }
    },
    {
      coordinate: { latitude: 42.39194359454453, longitude: -72.52466259261561 },
      id: 2,
      title: 'Hasbrouck',
      pinColor: 'red',
      description: 'I am currently coding this while in my Physics lab',
      onPress: (e) => { e.stopPropagation(); setOpen(2); }
    }
  ];


  const [state, setState] = useState(getInitialRegion());
  const [open, setOpen] = useState<number | null>(null)

  return (
    <View style={styles.container}>
      <MapView style={styles.map} region={state} onRegionChange={onRegionChange} onPress={() => setOpen(null)}> 
        {
          markers.map((m) => (
            <Marker
              key={m.id}
              coordinate={m.coordinate}
              pinColor={m.pinColor}
              onPress={m.onPress}
            />
          ))
        }
      </MapView>

      {open && (
        <View style={styles.markerExpanded}>
          <Text>{markers[open - 1].title}</Text>
          <Text>{markers[open - 1].description}</Text>
          <Pressable style={styles.markerExitButton} onPress={() => setOpen(null)}>
            <Text style={styles.markerExitText}>X</Text>
          </Pressable>
        </View>
      )}
      
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
  markerExpanded: {
    position: 'absolute',
    height: '50%',
    width: '100%',
    backgroundColor: 'white',
    bottom: 0,
    left: 0,
    right: 0,
    padding: 10,
    borderTopLeftRadius: 20,
    borderTopRightRadius: 20
  },
  markerExitButton: {
    backgroundColor: 'grey',
    padding: 5,
    alignContent: 'center',
    justifyContent: 'center',
    textAlign: 'center',
    width: 30,
    height: 30,
    borderRadius: 20,
    position: 'absolute',
    right: 10,
    top: 10,
  }, 
  markerExitText: {
    alignItems: 'center',
    justifyContent: 'center',
    textAlign: 'center'
  }
});