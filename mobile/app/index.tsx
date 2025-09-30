import React, { useState } from 'react';
import MapView, { Marker, Region } from 'react-native-maps';
import { StyleSheet, View, Text, Pressable } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

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
    tag: string,
    details: { address: string, relatedArtifacts: string[], qrcode: string}
    onPress: (e: any) => void
  }

  const markers: markerData[] = [
    {
      coordinate: { latitude: 42.39674910914364, longitude: -72.52415362726558 },
      id: 1,
      title: 'North B',
      pinColor: 'red',
      description: 'This is where Felix lives',
      tag: 'Residential Building',
      details: { address: '58 EastMan Ln.', relatedArtifacts: ['dddd', 'aaaa'], qrcode: 'FEDCBA'},
      onPress: (e) => { e.stopPropagation(); setOpen(1); }
    },
    {
      coordinate: { latitude: 42.39194359454453, longitude: -72.52466259261561 },
      id: 2,
      title: 'Hasbrouck',
      pinColor: 'red',
      description: 'Hasbrouck Laboratory, built in 1950, was the first major building placed in the campus center near North Pleasant Street, between Stockbridge Road and Ellis (Olmsted) Drive and adjacent to the Goessmann Lab precinct. Its siting marked a shift from preserving the central lowlands as pastoral open space to developing them, likely to showcase the university’s growing emphasis on scientific advancement.',
      tag: 'Academic Building',
      details: { address: '666 N Pleasant St.', relatedArtifacts: ['blah' , 'swdcfwf', 'dddd'], qrcode: 'ABCDEF'},
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
        <SafeAreaView style={styles.markerExpandedContainer}>
          <View style={styles.markerExpandedTitleAndExitContainer}>
            <Text style={styles.markerExpandedTitle}>{markers[open - 1].title}</Text>
            <Pressable style={styles.markerExpandedExitButton} onPress={() => setOpen(null)}>
              <Text style={styles.markerExpandedExitButtonText}>X</Text>
            </Pressable>
          </View>
          
          <Text style={styles.markerExpandedTag}>{markers[open - 1].tag}</Text>
          <Text style={styles.markerExpandedDescription}>{markers[open - 1].description}</Text>
          
          <View style={styles.markerExpandedDetailsContainer}>
            <Text style={styles.markerExpandedDetailsTitle}>Details</Text>
            <Text style={styles.markerExpandedDetailsText}>- {markers[open - 1].details.address}</Text>
            <Text style={styles.markerExpandedDetailsText}>- {markers[open - 1].details.relatedArtifacts.length} related artifacts</Text>
            <Text style={styles.markerExpandedDetailsText}>- QR Code: {markers[open - 1].details.qrcode}</Text>
          </View>
          
        </SafeAreaView>
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
  markerExpandedContainer: {
    position: 'absolute',
    height: '100%',
    width: '100%',
    backgroundColor: 'white',
    bottom: 0,
    left: 0,
    right: 0,
    padding: 10,
    borderTopLeftRadius: 20,
    borderTopRightRadius: 20,
    display: 'flex',
    flexDirection: 'column',
  },
  markerExpandedTitleAndExitContainer: {
    width: '100%',
    height: 'auto',
    display: 'flex',
    flexDirection: 'row',
    position: 'relative',
  },
  markerExpandedTitle: {
    fontWeight: 600,
    fontSize: 25,
  },
  markerExpandedExitButton: {
    backgroundColor: 'grey',
    padding: 5,
    alignContent: 'center',
    justifyContent: 'center',
    textAlign: 'center',
    width: 30,
    height: 30,
    borderRadius: 20,
    position: 'absolute',
    right: 3,
  }, 
  markerExpandedExitButtonText: {
    alignItems: 'center',
    justifyContent: 'center',
    textAlign: 'center'
  },
  markerExpandedTag: {
    marginTop: 5,
    fontWeight: 600,
    color: 'grey',
  },
  markerExpandedDescription: {
    marginTop: 5,
  },
  markerExpandedDetailsContainer: {
    marginTop: 10,
  },
  markerExpandedDetailsTitle: {
    fontSize: 20,
    fontWeight: 600,
  },
  markerExpandedDetailsText: {
      marginTop: 5,
      fontWeight: 400,
    }

});