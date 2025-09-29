import React, { useState } from 'react';
import {
  View,
  Text,
  StyleSheet,
  ScrollView,
  TouchableOpacity,
  Image,
  FlatList,
  Alert,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import { mockTours, mockSites } from '../data/mockData';
import type { Tour, Site } from '../types/types';

interface ToursViewProps {
  tours?: Tour[];
  sites?: Site[];
  onCreateTour?: (tour: Tour) => void;
  onSiteSelect?: (site: Site) => void;
}

export default function ToursView({ 
  tours = mockTours, 
  sites = mockSites, 
  onCreateTour,
  onSiteSelect 
}: ToursViewProps) {
  const [selectedTab, setSelectedTab] = useState<'available' | 'custom'>('available');

  const getDifficultyColor = (difficulty: string) => {
    switch (difficulty) {
      case 'Easy': return '#10b981'; // green
      case 'Moderate': return '#f59e0b'; // yellow
      case 'Challenging': return '#ef4444'; // red
      default: return '#6b7280'; // gray
    }
  };

  const getDifficultyIcon = (difficulty: string) => {
    switch (difficulty) {
      case 'Easy': return 'leaf-outline';
      case 'Moderate': return 'trail-sign-outline';
      case 'Challenging': return 'mountain-outline';
      default: return 'help-outline';
    }
  };

  const formatDuration = (minutes: number) => {
    if (minutes < 60) {
      return `${minutes}min`;
    } else {
      const hours = Math.floor(minutes / 60);
      const mins = minutes % 60;
      return mins > 0 ? `${hours}h ${mins}min` : `${hours}h`;
    }
  };

  const formatDistance = (meters: number) => {
    if (meters < 1000) {
      return `${meters}m`;
    } else {
      const km = (meters / 1000).toFixed(1);
      return `${km}km`;
    }
  };

  const getSitesForTour = (tour: Tour) => {
    return tour.siteIds.map(siteId => sites.find(site => site.id === siteId)).filter(Boolean) as Site[];
  };

  const getFirstSiteImage = (tour: Tour) => {
    const tourSites = getSitesForTour(tour);
    if (tourSites.length > 0) {
      const firstSite = tourSites[0];
      const imageArtifact = firstSite.artifacts.find(artifact => artifact.type === 'image');
      return imageArtifact?.content;
    }
    return null;
  };

  const handleStartTour = (tour: Tour) => {
    Alert.alert(
      'Start Tour',
      `Would you like to start "${tour.name}"?`,
      [
        { text: 'Cancel', style: 'cancel' },
        { 
          text: 'Start', 
          onPress: () => {
            // Navigate to first site or tour mode
            const tourSites = getSitesForTour(tour);
            if (tourSites.length > 0 && onSiteSelect) {
              onSiteSelect(tourSites[0]);
            }
          }
        }
      ]
    );
  };

  const handleCreateCustomTour = () => {
    Alert.alert(
      'Create Custom Tour',
      'Custom tour creation feature coming soon!',
      [{ text: 'OK' }]
    );
  };

  const renderTourCard = ({ item: tour }: { item: Tour }) => {
    const tourSites = getSitesForTour(tour);
    const firstImage = getFirstSiteImage(tour);

    return (
      <TouchableOpacity
        style={styles.tourCard}
        onPress={() => handleStartTour(tour)}
        activeOpacity={0.7}
      >
        <View style={styles.tourCardContent}>
          {/* Tour Image */}
          <View style={styles.tourImageContainer}>
            {firstImage ? (
              <Image
                source={{ uri: firstImage }}
                style={styles.tourImage}
                resizeMode="cover"
              />
            ) : (
              <View style={styles.fallbackImage}>
                <Ionicons name="map-outline" size={32} color="#9ca3af" />
              </View>
            )}
            
            {/* Difficulty Badge */}
            <View style={[styles.difficultyBadge, { backgroundColor: getDifficultyColor(tour.difficulty) }]}>
              <Ionicons 
                name={getDifficultyIcon(tour.difficulty) as any} 
                size={12} 
                color="#ffffff" 
              />
              <Text style={styles.difficultyText}>{tour.difficulty}</Text>
            </View>
          </View>

          {/* Tour Info */}
          <View style={styles.tourInfo}>
            <Text style={styles.tourName} numberOfLines={1}>{tour.name}</Text>
            <Text style={styles.tourDescription} numberOfLines={2}>
              {tour.description}
            </Text>
            
            {/* Tour Stats */}
            <View style={styles.tourStats}>
              <View style={styles.statItem}>
                <Ionicons name="time-outline" size={14} color="#9ca3af" />
                <Text style={styles.statText}>{formatDuration(tour.estimatedDuration)}</Text>
              </View>
              
              <View style={styles.statItem}>
                <Ionicons name="walk-outline" size={14} color="#9ca3af" />
                <Text style={styles.statText}>{formatDistance(tour.totalDistance)}</Text>
              </View>
              
              <View style={styles.statItem}>
                <Ionicons name="location-outline" size={14} color="#9ca3af" />
                <Text style={styles.statText}>{tourSites.length} sites</Text>
              </View>
            </View>

            {/* Sites Preview */}
            <View style={styles.sitesPreview}>
              <Text style={styles.sitesLabel}>Sites included:</Text>
              <ScrollView 
                horizontal 
                showsHorizontalScrollIndicator={false}
                style={styles.sitesScroll}
              >
                {tourSites.map((site, index) => (
                  <TouchableOpacity
                    key={site.id}
                    style={styles.siteChip}
                    onPress={() => onSiteSelect?.(site)}
                    activeOpacity={0.7}
                  >
                    <Text style={styles.siteChipText} numberOfLines={1}>
                      {index + 1}. {site.name}
                    </Text>
                  </TouchableOpacity>
                ))}
              </ScrollView>
            </View>
          </View>
        </View>

        {/* Start Tour Button */}
        <View style={styles.tourCardFooter}>
          <TouchableOpacity 
            style={styles.startButton}
            onPress={() => handleStartTour(tour)}
            activeOpacity={0.8}
          >
            <Ionicons name="play-outline" size={16} color="#ffffff" />
            <Text style={styles.startButtonText}>Start Tour</Text>
          </TouchableOpacity>
        </View>
      </TouchableOpacity>
    );
  };

  const renderEmptyState = () => (
    <View style={styles.emptyState}>
      <Ionicons name="map-outline" size={64} color="#4b5563" />
      <Text style={styles.emptyTitle}>No Custom Tours</Text>
      <Text style={styles.emptySubtitle}>Create your first custom tour to get started</Text>
      <TouchableOpacity 
        style={styles.createTourButton}
        onPress={handleCreateCustomTour}
        activeOpacity={0.8}
      >
        <Ionicons name="add-outline" size={20} color="#ffffff" />
        <Text style={styles.createTourButtonText}>Create Tour</Text>
      </TouchableOpacity>
    </View>
  );

  const availableTours = tours.filter(tour => !tour.isCustom);
  const customTours = tours.filter(tour => tour.isCustom);

  return (
    <SafeAreaView style={styles.container}>
      {/* Header */}
      <View style={styles.header}>
        <Text style={styles.title}>Tours</Text>
        <Text style={styles.subtitle}>
          Discover guided experiences through campus sites
        </Text>
        
        {/* Tab Navigation */}
        <View style={styles.tabContainer}>
          <TouchableOpacity
            style={[styles.tab, selectedTab === 'available' && styles.activeTab]}
            onPress={() => setSelectedTab('available')}
            activeOpacity={0.7}
          >
            <Text style={[styles.tabText, selectedTab === 'available' && styles.activeTabText]}>
              Available Tours
            </Text>
          </TouchableOpacity>
          
          <TouchableOpacity
            style={[styles.tab, selectedTab === 'custom' && styles.activeTab]}
            onPress={() => setSelectedTab('custom')}
            activeOpacity={0.7}
          >
            <Text style={[styles.tabText, selectedTab === 'custom' && styles.activeTabText]}>
              Custom Tours
            </Text>
          </TouchableOpacity>
        </View>
      </View>

      {/* Tours Content */}
      <View style={styles.content}>
        {selectedTab === 'available' ? (
          <>
            <View style={styles.sectionHeader}>
              <Text style={styles.sectionTitle}>
                {availableTours.length} Available Tour{availableTours.length !== 1 ? 's' : ''}
              </Text>
            </View>
            
            <FlatList
              data={availableTours}
              renderItem={renderTourCard}
              keyExtractor={(item) => item.id}
              showsVerticalScrollIndicator={false}
              contentContainerStyle={styles.toursList}
            />
          </>
        ) : (
          <>
            <View style={styles.sectionHeader}>
              <Text style={styles.sectionTitle}>Your Custom Tours</Text>
              <TouchableOpacity 
                style={styles.addButton}
                onPress={handleCreateCustomTour}
                activeOpacity={0.8}
              >
                <Ionicons name="add-outline" size={20} color="#3b82f6" />
              </TouchableOpacity>
            </View>
            
            {customTours.length > 0 ? (
              <FlatList
                data={customTours}
                renderItem={renderTourCard}
                keyExtractor={(item) => item.id}
                showsVerticalScrollIndicator={false}
                contentContainerStyle={styles.toursList}
              />
            ) : (
              renderEmptyState()
            )}
          </>
        )}
      </View>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#0f172a', // slate-900
  },
  header: {
    padding: 16,
    borderBottomWidth: 1,
    borderBottomColor: '#374151', // gray-700
  },
  title: {
    fontSize: 20,
    fontWeight: 'bold',
    color: '#ffffff',
  },
  subtitle: {
    fontSize: 14,
    color: '#9ca3af',
    marginTop: 4,
    marginBottom: 16,
  },
  tabContainer: {
    flexDirection: 'row',
    backgroundColor: '#1e293b', // slate-800
    borderRadius: 8,
    padding: 4,
  },
  tab: {
    flex: 1,
    paddingVertical: 8,
    paddingHorizontal: 12,
    borderRadius: 6,
    alignItems: 'center',
  },
  activeTab: {
    backgroundColor: '#3b82f6',
  },
  tabText: {
    fontSize: 14,
    fontWeight: '500',
    color: '#9ca3af',
  },
  activeTabText: {
    color: '#ffffff',
  },
  content: {
    flex: 1,
    padding: 16,
  },
  sectionHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 16,
  },
  sectionTitle: {
    fontSize: 16,
    fontWeight: '600',
    color: '#ffffff',
  },
  addButton: {
    width: 32,
    height: 32,
    borderRadius: 16,
    backgroundColor: 'rgba(59, 130, 246, 0.1)',
    borderWidth: 1,
    borderColor: '#3b82f6',
    justifyContent: 'center',
    alignItems: 'center',
  },
  toursList: {
    paddingBottom: 20,
  },
  tourCard: {
    backgroundColor: '#1e293b', // slate-800
    borderWidth: 1,
    borderColor: '#374151', // gray-700
    borderRadius: 12,
    marginBottom: 16,
    overflow: 'hidden',
  },
  tourCardContent: {
    padding: 16,
  },
  tourImageContainer: {
    position: 'relative',
    marginBottom: 16,
  },
  tourImage: {
    width: '100%',
    height: 160,
    borderRadius: 8,
  },
  fallbackImage: {
    width: '100%',
    height: 160,
    backgroundColor: '#374151',
    borderRadius: 8,
    justifyContent: 'center',
    alignItems: 'center',
  },
  difficultyBadge: {
    position: 'absolute',
    top: 8,
    right: 8,
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 12,
    gap: 4,
  },
  difficultyText: {
    color: '#ffffff',
    fontSize: 12,
    fontWeight: '600',
  },
  tourInfo: {
    gap: 8,
  },
  tourName: {
    fontSize: 18,
    fontWeight: '600',
    color: '#ffffff',
  },
  tourDescription: {
    fontSize: 14,
    color: '#9ca3af',
    lineHeight: 20,
  },
  tourStats: {
    flexDirection: 'row',
    gap: 16,
  },
  statItem: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
  },
  statText: {
    fontSize: 12,
    color: '#9ca3af',
  },
  sitesPreview: {
    marginTop: 8,
  },
  sitesLabel: {
    fontSize: 12,
    color: '#9ca3af',
    marginBottom: 8,
  },
  sitesScroll: {
    flexDirection: 'row',
  },
  siteChip: {
    backgroundColor: '#374151',
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 16,
    marginRight: 8,
  },
  siteChipText: {
    fontSize: 12,
    color: '#ffffff',
    fontWeight: '500',
  },
  tourCardFooter: {
    borderTopWidth: 1,
    borderTopColor: '#374151',
    padding: 16,
  },
  startButton: {
    backgroundColor: '#3b82f6',
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 12,
    paddingHorizontal: 16,
    borderRadius: 8,
    gap: 8,
  },
  startButtonText: {
    color: '#ffffff',
    fontSize: 14,
    fontWeight: '600',
  },
  emptyState: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    paddingVertical: 48,
  },
  emptyTitle: {
    fontSize: 18,
    color: '#9ca3af',
    marginTop: 16,
    marginBottom: 8,
  },
  emptySubtitle: {
    fontSize: 14,
    color: '#6b7280',
    textAlign: 'center',
    marginBottom: 24,
  },
  createTourButton: {
    backgroundColor: '#3b82f6',
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 20,
    paddingVertical: 12,
    borderRadius: 8,
    gap: 8,
  },
  createTourButtonText: {
    color: '#ffffff',
    fontSize: 14,
    fontWeight: '600',
  },
});
