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


  const [state, setState] = useState(getInitialRegion());
  const [open, setOpen] = useState(false)

  return (
    <View style={styles.container}>
      <MapView style={styles.map} region={state} onRegionChange={onRegionChange} onPress={() => setOpen(false)}> 
        <Marker coordinate={{ latitude: 42.39674910914364, longitude: -72.52415362726558 }} title='North B' pinColor='red' description='This is where Felix lives' onPress={ (e) => { e.stopPropagation(); setOpen(true); } }/>
      </MapView>

      {open && (
        <View style={styles.markerExpanded}>
          <Text>North B</Text>
          <Pressable style={styles.markerExitButton} onPress={() => setOpen(false)}>
            <Text>X</Text>
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
    position:'absolute',
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
    width: 30,
    height: 30,
    borderRadius: 20
  }
});