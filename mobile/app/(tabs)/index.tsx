import { StyleSheet, Text, TouchableOpacity, View } from 'react-native';
import MapView, { Callout, Marker } from 'react-native-maps';
import { useRouter } from 'expo-router';
import { listArtifacts } from '../../api';

const pins = listArtifacts().filter(a => a.lat != null);

export default function MapScreen() {
  const router = useRouter();
  return (
    <View style={styles.container}>
      <MapView
        style={styles.map}
        initialRegion={{
          latitude: 42.39170,
          longitude: -72.52710,
          latitudeDelta: 0.02,
          longitudeDelta: 0.02,
        }}
      >
        {pins.map(a => (
          <Marker
            key={a.id}
            coordinate={{ latitude: a.lat!, longitude: a.lng! }}
            pinColor="#c97d3a"
          >
            <Callout tooltip onPress={() => router.push(`/${a.id}`)}>
              <TouchableOpacity style={styles.callout} activeOpacity={0.85}>
                <Text style={styles.calloutTitle}>{a.name}</Text>
                {a.desc ? (
                  <Text style={styles.calloutDesc} numberOfLines={2}>
                    {a.desc}
                  </Text>
                ) : null}
                <Text style={styles.calloutCta}>View site →</Text>
              </TouchableOpacity>
            </Callout>
          </Marker>
        ))}
      </MapView>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1 },
  map: { flex: 1 },
  callout: {
    width: 220,
    backgroundColor: '#221e1a',
    borderRadius: 12,
    padding: 14,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.35,
    shadowRadius: 8,
    elevation: 6,
  },
  calloutTitle: {
    color: '#e8e0d8',
    fontSize: 15,
    fontWeight: '700',
    marginBottom: 4,
  },
  calloutDesc: {
    color: '#9e9690',
    fontSize: 13,
    lineHeight: 18,
    marginBottom: 10,
  },
  calloutCta: {
    color: '#c97d3a',
    fontSize: 13,
    fontWeight: '600',
  },
});
