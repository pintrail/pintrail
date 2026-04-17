import { Image, ScrollView, StyleSheet, Text, TouchableOpacity, View } from 'react-native';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { getArtifact } from '../api';

export default function DetailScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const router = useRouter();
  const artifact = getArtifact(id ?? '');

  if (!artifact) {
    return (
      <View style={styles.center}>
        <Stack.Screen options={{ title: 'Not Found' }} />
        <Text style={styles.errorText}>Site not found</Text>
      </View>
    );
  }

  const children = artifact.children ?? [];

  return (
    <ScrollView style={styles.scroll} contentContainerStyle={styles.content}>
      <Stack.Screen options={{ title: artifact.name }} />

      {artifact.image_urls.length > 0 && (
        <ScrollView horizontal showsHorizontalScrollIndicator={false} style={styles.gallery}>
          {artifact.image_urls.map((url, i) => (
            <Image key={i} source={{ uri: url }} style={styles.image} />
          ))}
        </ScrollView>
      )}

      <View style={styles.body}>
        <Text style={styles.title}>{artifact.name}</Text>
        <Text style={styles.desc}>{artifact.desc}</Text>

        {artifact.lat != null && (
          <View style={styles.section}>
            <Text style={styles.label}>LOCATION</Text>
            <Text style={styles.coords}>
              {artifact.lat.toFixed(6)},  {artifact.lng!.toFixed(6)}
            </Text>
          </View>
        )}

        {children.length > 0 && (
          <View style={styles.section}>
            <Text style={styles.label}>CONNECTED SITES</Text>
            {children.map((child, i) => (
              <TouchableOpacity
                key={child.id}
                style={[styles.childRow, i === children.length - 1 && styles.childRowLast]}
                onPress={() => router.push(`/${child.id}`)}
              >
                <Text style={styles.childName}>{child.name}</Text>
                <Text style={styles.chevron}>›</Text>
              </TouchableOpacity>
            ))}
          </View>
        )}
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  scroll: { flex: 1, backgroundColor: '#1a1714' },
  content: { paddingBottom: 40 },
  center: { flex: 1, justifyContent: 'center', alignItems: 'center', backgroundColor: '#1a1714' },
  errorText: { color: '#9e9690', fontSize: 15 },
  gallery: { marginBottom: 0 },
  image: { width: 320, height: 210, marginRight: 2 },
  body: { padding: 20 },
  title: { color: '#e8e0d8', fontSize: 26, fontWeight: '700', marginBottom: 10 },
  desc: { color: '#9e9690', fontSize: 15, lineHeight: 23 },
  section: { marginTop: 28 },
  label: { color: '#c97d3a', fontSize: 11, fontWeight: '700', letterSpacing: 1, marginBottom: 10 },
  coords: { color: '#e8e0d8', fontSize: 14 },
  childRow: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: 13,
    borderBottomWidth: 1,
    borderBottomColor: 'rgba(255,255,255,0.07)',
  },
  childRowLast: { borderBottomWidth: 0 },
  childName: { flex: 1, color: '#e8e0d8', fontSize: 15 },
  chevron: { color: '#9e9690', fontSize: 22 },
});
