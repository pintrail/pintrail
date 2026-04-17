import { FlatList, Image, ScrollView, StyleSheet, Text, TouchableOpacity, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useRouter } from 'expo-router';
import { useState } from 'react';
import { listArtifacts, Artifact } from '../../api';

const artifacts = listArtifacts();

const ALL_TAGS = ['All', ...Array.from(new Set(artifacts.flatMap(a => a.tags))).sort()];

function TagPill({ label, active, onPress }: { label: string; active: boolean; onPress: () => void }) {
  return (
    <TouchableOpacity style={[styles.pill, active && styles.pillActive]} onPress={onPress} activeOpacity={0.7}>
      <Text style={[styles.pillText, active && styles.pillTextActive]}>{label}</Text>
    </TouchableOpacity>
  );
}

function SiteRow({ item }: { item: Artifact }) {
  const router = useRouter();
  const thumb = item.image_urls[0];
  return (
    <TouchableOpacity style={styles.row} onPress={() => router.push(`/${item.id}`)} activeOpacity={0.75}>
      {thumb ? (
        <Image source={{ uri: thumb }} style={styles.thumb} />
      ) : (
        <View style={[styles.thumb, styles.thumbPlaceholder]} />
      )}
      <View style={styles.rowContent}>
        <Text style={styles.name} numberOfLines={1}>{item.name}</Text>
        <Text style={styles.desc} numberOfLines={2}>{item.desc}</Text>
        {item.tags.length > 0 && (
          <View style={styles.tagRow}>
            {item.tags.map(t => (
              <View key={t} style={styles.tag}>
                <Text style={styles.tagText}>{t}</Text>
              </View>
            ))}
          </View>
        )}
      </View>
      <Text style={styles.chevron}>›</Text>
    </TouchableOpacity>
  );
}

export default function ListScreen() {
  const insets = useSafeAreaInsets();
  const [active, setActive] = useState('All');

  const filtered = active === 'All' ? artifacts : artifacts.filter(a => a.tags.includes(active));

  return (
    <View style={[styles.root, { paddingTop: insets.top }]}>
      <ScrollView
        horizontal
        showsHorizontalScrollIndicator={false}
        style={styles.filterBar}
        contentContainerStyle={styles.filterBarContent}
      >
        {ALL_TAGS.map(t => (
          <TagPill key={t} label={t} active={active === t} onPress={() => setActive(t)} />
        ))}
      </ScrollView>

      <FlatList
        data={filtered}
        keyExtractor={a => a.id}
        renderItem={({ item }) => <SiteRow item={item} />}
        ItemSeparatorComponent={() => <View style={styles.separator} />}
        contentContainerStyle={filtered.length === 0 && styles.emptyContainer}
        ListEmptyComponent={<Text style={styles.emptyText}>No sites match this filter.</Text>}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: '#1a1714' },
  filterBar: { flexGrow: 0, backgroundColor: '#1a1714' },
  filterBarContent: { paddingHorizontal: 14, paddingVertical: 10, gap: 8 },
  pill: {
    paddingHorizontal: 14,
    paddingVertical: 6,
    borderRadius: 20,
    backgroundColor: '#2e2824',
    borderWidth: 1,
    borderColor: 'rgba(255,255,255,0.08)',
  },
  pillActive: { backgroundColor: '#c97d3a', borderColor: '#c97d3a' },
  pillText: { color: '#9e9690', fontSize: 13, fontWeight: '600' },
  pillTextActive: { color: '#fff' },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 16,
    paddingVertical: 12,
    backgroundColor: '#221e1a',
    gap: 12,
  },
  thumb: { width: 56, height: 56, borderRadius: 10 },
  thumbPlaceholder: { backgroundColor: '#2e2824' },
  rowContent: { flex: 1, gap: 3 },
  name: { color: '#e8e0d8', fontSize: 15, fontWeight: '600' },
  desc: { color: '#9e9690', fontSize: 13, lineHeight: 18 },
  tagRow: { flexDirection: 'row', flexWrap: 'wrap', gap: 5, marginTop: 4 },
  tag: {
    backgroundColor: '#2e2824',
    borderRadius: 6,
    paddingHorizontal: 7,
    paddingVertical: 2,
  },
  tagText: { color: '#c97d3a', fontSize: 11, fontWeight: '600' },
  chevron: { color: '#9e9690', fontSize: 22 },
  separator: { height: 1, backgroundColor: 'rgba(255,255,255,0.07)', marginLeft: 84 },
  emptyContainer: { flex: 1, justifyContent: 'center', alignItems: 'center' },
  emptyText: { color: '#9e9690', fontSize: 15 },
});
