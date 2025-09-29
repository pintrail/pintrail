import React from 'react';
import {
  StyleSheet,
  View,
  Text,
  TouchableOpacity,
  ScrollView
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import { useLocalSearchParams, router } from 'expo-router';
import { mockSites } from '../data/mockData';

export default function DetailPage() {
  const { siteId } = useLocalSearchParams<{ siteId: string }>();
  
  // Find the site by ID
  const site = mockSites.find(s => s.id === siteId);

  if (!site) {
    return (
      <SafeAreaView style={styles.container}>
        <View style={styles.header}>
          <TouchableOpacity
            onPress={() => router.back()}
            style={styles.backButton}
          >
            <Ionicons name="arrow-back" size={24} color="#374151" />
          </TouchableOpacity>
          <Text style={styles.title}>Site Not Found</Text>
        </View>
        <View style={styles.errorContainer}>
          <Text style={styles.errorText}>The requested site could not be found.</Text>
        </View>
      </SafeAreaView>
    );
  }

  return (
    <SafeAreaView style={styles.container}>
      <View style={styles.header}>
        <TouchableOpacity
          onPress={() => router.back()}
          style={styles.backButton}
        >
          <Ionicons name="arrow-back" size={24} color="#374151" />
        </TouchableOpacity>
        <Text style={styles.title} numberOfLines={1}>
          {site.name}
        </Text>
      </View>

      <ScrollView style={styles.content} showsVerticalScrollIndicator={false}>
        <Text style={styles.categoryText}>
          {site.category}
        </Text>
        
        <Text style={styles.descriptionText}>
          {site.description}
        </Text>

        <View style={styles.detailsSection}>
          <Text style={styles.sectionTitle}>Details</Text>
          
          <View style={styles.detailRow}>
            <Ionicons name="location-outline" size={20} color="#6b7280" />
            <Text style={styles.detailText}>{site.address}</Text>
          </View>

          {site.walkingTime !== undefined && site.walkingTime > 0 && (
            <View style={styles.detailRow}>
              <Ionicons name="time-outline" size={20} color="#6b7280" />
              <Text style={styles.detailText}>{site.walkingTime} minutes walk</Text>
            </View>
          )}

          <View style={styles.detailRow}>
            <Ionicons name="albums-outline" size={20} color="#6b7280" />
            <Text style={styles.detailText}>
              {site.artifacts.length} artifact{site.artifacts.length !== 1 ? 's' : ''} available
            </Text>
          </View>

          {site.qrCode && (
            <View style={styles.detailRow}>
              <Ionicons name="qr-code-outline" size={20} color="#6b7280" />
              <Text style={styles.detailText}>QR Code: {site.qrCode}</Text>
            </View>
          )}
        </View>

        {site.artifacts.length > 0 && (
          <View style={styles.artifactsSection}>
            <Text style={styles.sectionTitle}>Artifacts</Text>
            {site.artifacts.map((artifact) => (
              <View key={artifact.id} style={styles.artifactItem}>
                <View style={styles.artifactHeader}>
                  <Ionicons 
                    name={
                      artifact.type === 'image' ? 'image-outline' :
                      artifact.type === 'audio' ? 'musical-note-outline' :
                      artifact.type === 'video' ? 'videocam-outline' :
                      artifact.type === 'document' ? 'document-outline' :
                      'text-outline'
                    } 
                    size={16} 
                    color="#6b7280" 
                  />
                  <Text style={styles.artifactTitle}>{artifact.title}</Text>
                  <View style={styles.artifactTypeBadge}>
                    <Text style={styles.artifactTypeText}>{artifact.type}</Text>
                  </View>
                </View>
                {artifact.description && (
                  <Text style={styles.artifactDescription}>{artifact.description}</Text>
                )}
              </View>
            ))}
          </View>
        )}

        {site.connectedSites.length > 0 && (
          <View style={styles.connectedSection}>
            <Text style={styles.sectionTitle}>Connected Sites</Text>
            {site.connectedSites.map((connectedId) => {
              const connectedSite = mockSites.find(s => s.id === connectedId);
              if (!connectedSite) return null;
              
              return (
                <TouchableOpacity 
                  key={connectedId} 
                  style={styles.connectedSiteItem}
                  onPress={() => router.push({ pathname: '/detail', params: { siteId: connectedId } })}
                >
                  <View style={styles.connectedSiteInfo}>
                    <Text style={styles.connectedSiteName}>{connectedSite.name}</Text>
                    <Text style={styles.connectedSiteCategory}>{connectedSite.category}</Text>
                  </View>
                  <Ionicons name="chevron-forward" size={20} color="#9ca3af" />
                </TouchableOpacity>
              );
            })}
          </View>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#ffffff',
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    padding: 16,
    borderBottomWidth: 1,
    borderBottomColor: '#e5e7eb',
  },
  backButton: {
    padding: 8,
    marginRight: 8,
  },
  title: {
    fontSize: 20,
    fontWeight: 'bold',
    color: '#111827',
    flex: 1,
  },
  content: {
    flex: 1,
    padding: 16,
  },
  categoryText: {
    fontSize: 14,
    fontWeight: '600',
    color: '#6b7280',
    marginBottom: 8,
  },
  descriptionText: {
    fontSize: 16,
    color: '#374151',
    lineHeight: 24,
    marginBottom: 24,
  },
  detailsSection: {
    marginBottom: 24,
  },
  sectionTitle: {
    fontSize: 18,
    fontWeight: 'bold',
    color: '#111827',
    marginBottom: 16,
  },
  detailRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    marginBottom: 12,
  },
  detailText: {
    fontSize: 16,
    color: '#374151',
    flex: 1,
  },
  artifactsSection: {
    marginBottom: 24,
  },
  artifactItem: {
    backgroundColor: '#f9fafb',
    borderRadius: 8,
    padding: 12,
    marginBottom: 8,
  },
  artifactHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    marginBottom: 4,
  },
  artifactTitle: {
    fontSize: 14,
    fontWeight: '600',
    color: '#374151',
    flex: 1,
  },
  artifactTypeBadge: {
    backgroundColor: '#e5e7eb',
    paddingHorizontal: 6,
    paddingVertical: 2,
    borderRadius: 4,
  },
  artifactTypeText: {
    fontSize: 10,
    fontWeight: '500',
    color: '#6b7280',
    textTransform: 'uppercase',
  },
  artifactDescription: {
    fontSize: 12,
    color: '#6b7280',
    marginLeft: 24,
  },
  connectedSection: {
    marginBottom: 24,
  },
  connectedSiteItem: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#f9fafb',
    borderRadius: 8,
    padding: 12,
    marginBottom: 8,
  },
  connectedSiteInfo: {
    flex: 1,
  },
  connectedSiteName: {
    fontSize: 14,
    fontWeight: '600',
    color: '#374151',
    marginBottom: 2,
  },
  connectedSiteCategory: {
    fontSize: 12,
    color: '#6b7280',
  },
  errorContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    padding: 16,
  },
  errorText: {
    fontSize: 16,
    color: '#6b7280',
    textAlign: 'center',
  },
});
