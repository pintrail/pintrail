import React, { useState, useRef, useMemo, useCallback } from 'react';
import MapView, { Region, Marker } from 'react-native-maps';
import {
  StyleSheet,
  View,
  Text,
  TouchableOpacity,
  ScrollView,
} from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import BottomSheet, { BottomSheetView, BottomSheetFlatList } from '@gorhom/bottom-sheet';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { mockSites } from '../data/mockData';
import type { Site } from '../types/types';

export default function MainView() {
  const [region, setRegion] = useState({
    latitude: 42.391702,
    longitude: -72.527101,
    latitudeDelta: 0.01,
    longitudeDelta: 0.01,
  });

  const [selectedSite, setSelectedSite] = useState<Site | null>(null);

  // Bottom sheet refs and snap points
  const bottomSheetRef = useRef<BottomSheet>(null);
  const snapPoints = useMemo(() => ['12%', '35%', '85%'], []);

  const onRegionChange = (region: Region) => {
    setRegion(region);
  };

  const handleMarkerPress = useCallback((site: Site) => {
    setSelectedSite(site);
    // Animate to mid height when a site is selected
    bottomSheetRef.current?.snapToIndex(1);
  }, []);

  const handleSheetChanges = useCallback((index: number) => {
    // Handle sheet changes if needed
  }, []);

  const getCategoryColor = (category: string) => {
    switch (category) {
      case 'Academic Building': return '#ef4444';
      case 'Learning Center': return '#3b82f6';
      case 'Natural Site': return '#10b981';
      case 'Historic Route': return '#f59e0b';
      case 'Historic Building': return '#8b5cf6';
      default: return '#6b7280';
    }
  };

  const renderSiteItem = (site: Site, index: number) => (
    <TouchableOpacity
      key={site.id}
      style={[
        styles.siteItem,
        selectedSite?.id === site.id && styles.siteItemSelected
      ]}
      onPress={() => {
        setSelectedSite(site);
        // Center map on selected site
        setRegion({
          ...region,
          latitude: site.location.lat,
          longitude: site.location.lng,
        });
        // Snap to mid height to show details
        bottomSheetRef.current?.snapToIndex(1);
      }}
      activeOpacity={0.7}
    >
      <View style={styles.siteItemContent}>
        <View style={styles.siteHeader}>
          <View style={styles.siteNumber}>
            <Text style={styles.siteNumberText}>{index + 1}</Text>
          </View>
          <View style={styles.siteInfo}>
            <Text style={styles.siteName}>{site.name}</Text>
            <View style={[styles.categoryBadge, { backgroundColor: getCategoryColor(site.category) }]}>
              <Text style={styles.categoryText}>{site.category}</Text>
            </View>
          </View>
          {site.walkingTime !== undefined && site.walkingTime > 0 && (
            <View style={styles.walkingTime}>
              <Ionicons name="time-outline" size={12} color="#9ca3af" />
              <Text style={styles.walkingTimeText}>{site.walkingTime} min</Text>
            </View>
          )}
        </View>

        <Text style={styles.siteDescription} numberOfLines={2}>
          {site.description}
        </Text>

        <View style={styles.siteStats}>
          <View style={styles.statItem}>
            <Ionicons name="location-outline" size={12} color="#9ca3af" />
            <Text style={styles.statText}>{site.address}</Text>
          </View>
          <View style={styles.statItem}>
            <Ionicons name="albums-outline" size={12} color="#9ca3af" />
            <Text style={styles.statText}>{site.artifacts.length} artifacts</Text>
          </View>
          {site.qrCode && (
            <View style={styles.statItem}>
              <Ionicons name="qr-code-outline" size={12} color="#9ca3af" />
              <Text style={styles.statText}>{site.qrCode}</Text>
            </View>
          )}
        </View>
      </View>
    </TouchableOpacity>
  );

  return (
    <GestureHandlerRootView style={styles.container}>
      {/* Map */}
      <MapView
        style={styles.map}
        region={region}
        onRegionChange={onRegionChange}
        showsUserLocation={true}
        showsMyLocationButton={false}
      >
        {mockSites.map((site) => (
          <Marker
            key={site.id}
            coordinate={{
              latitude: site.location.lat,
              longitude: site.location.lng,
            }}
            onPress={() => handleMarkerPress(site)}
          >
            <View style={[styles.markerContainer, { backgroundColor: getCategoryColor(site.category) }]}>
              <Text style={styles.markerText}>{mockSites.indexOf(site) + 1}</Text>
            </View>
          </Marker>
        ))}
      </MapView>

      {/* Bottom Sheet */}
      <BottomSheet
        ref={bottomSheetRef}
        index={0}
        snapPoints={snapPoints}
        onChange={handleSheetChanges}
        enablePanDownToClose={false}
        handleIndicatorStyle={styles.sheetHandle}
        backgroundStyle={styles.bottomSheetBackground}
        enableDynamicSizing={false}
      >
        {selectedSite ? (
          // Selected Site Details
          <BottomSheetView style={styles.bottomSheetContent}>
            <View style={styles.sheetHeader}>
              <Text style={styles.sheetTitle}>{selectedSite.name}</Text>
              <Text style={styles.sheetSubtitle}>{selectedSite.category}</Text>
            </View>

            <View style={styles.selectedSiteContent}>
              <Text style={styles.selectedSiteDescription}>
                {selectedSite.description}
              </Text>

              <View style={styles.selectedSiteDetails}>
                <View style={styles.detailRow}>
                  <Ionicons name="location-outline" size={16} color="#9ca3af" />
                  <Text style={styles.detailText}>{selectedSite.address}</Text>
                </View>

                {selectedSite.walkingTime !== undefined && selectedSite.walkingTime > 0 && (
                  <View style={styles.detailRow}>
                    <Ionicons name="time-outline" size={16} color="#9ca3af" />
                    <Text style={styles.detailText}>{selectedSite.walkingTime} minutes walk</Text>
                  </View>
                )}

                <View style={styles.detailRow}>
                  <Ionicons name="albums-outline" size={16} color="#9ca3af" />
                  <Text style={styles.detailText}>
                    {selectedSite.artifacts.length} artifact{selectedSite.artifacts.length !== 1 ? 's' : ''} available
                  </Text>
                </View>

                {selectedSite.qrCode && (
                  <View style={styles.detailRow}>
                    <Ionicons name="qr-code-outline" size={16} color="#9ca3af" />
                    <Text style={styles.detailText}>QR Code: {selectedSite.qrCode}</Text>
                  </View>
                )}
              </View>

              <TouchableOpacity
                style={styles.viewAllButton}
                onPress={() => {
                  setSelectedSite(null);
                  bottomSheetRef.current?.snapToIndex(2);
                }}
                activeOpacity={0.7}
              >
                <Text style={styles.viewAllButtonText}>View All Sites</Text>
                <Ionicons name="list-outline" size={16} color="#3b82f6" />
              </TouchableOpacity>
            </View>
          </BottomSheetView>
        ) : (
          // All Sites List - Use BottomSheetFlatList for scrolling
          <BottomSheetFlatList
            data={mockSites}
            keyExtractor={(item: Site) => item.id}
            renderItem={({ item, index }: { item: Site, index: number }) => renderSiteItem(item, index)}
            ListHeaderComponent={() => (
              <View style={styles.sheetHeader}>
                <Text style={styles.sheetTitle}>Explore Sites</Text>
                <Text style={styles.sheetSubtitle}>{mockSites.length} sites available</Text>
              </View>
            )}
            contentContainerStyle={styles.flatListContent}
            showsVerticalScrollIndicator={false}
          />
        )}
      </BottomSheet>
    </GestureHandlerRootView>
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
  markerContainer: {
    width: 30,
    height: 30,
    borderRadius: 15,
    justifyContent: 'center',
    alignItems: 'center',
    borderWidth: 2,
    borderColor: '#ffffff',
  },
  markerText: {
    color: '#ffffff',
    fontSize: 12,
    fontWeight: 'bold',
  },
  bottomSheetBackground: {
    backgroundColor: '#ffffff',
    borderTopLeftRadius: 20,
    borderTopRightRadius: 20,
    shadowColor: '#000',
    shadowOffset: {
      width: 0,
      height: -2,
    },
    shadowOpacity: 0.1,
    shadowRadius: 10,
    elevation: 10,
  },
  bottomSheetContent: {
    flex: 1,
  },
  sheetHandle: {
    backgroundColor: '#d1d5db',
    width: 40,
    height: 4,
  },
  sheetHeader: {
    paddingHorizontal: 16,
    paddingBottom: 16,
    borderBottomWidth: 1,
    borderBottomColor: '#f3f4f6',
  },
  sheetTitle: {
    fontSize: 18,
    fontWeight: 'bold',
    color: '#1f2937',
    marginBottom: 4,
  },
  sheetSubtitle: {
    fontSize: 14,
    color: '#6b7280',
  },
  sheetContent: {
    flex: 1,
    paddingHorizontal: 16,
    paddingTop: 8,
  },
  selectedSiteContent: {
    paddingVertical: 16,
  },
  selectedSiteDescription: {
    fontSize: 16,
    color: '#374151',
    lineHeight: 24,
    marginBottom: 20,
  },
  selectedSiteDetails: {
    gap: 12,
    marginBottom: 20,
  },
  detailRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  detailText: {
    fontSize: 14,
    color: '#6b7280',
    flex: 1,
  },
  viewAllButton: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 8,
    backgroundColor: '#f8fafc',
    borderWidth: 1,
    borderColor: '#3b82f6',
    borderRadius: 8,
    paddingVertical: 12,
    paddingHorizontal: 16,
  },
  viewAllButtonText: {
    fontSize: 14,
    fontWeight: '600',
    color: '#3b82f6',
  },
  sitesContainer: {
    paddingVertical: 16,
    gap: 12,
  },
  siteItem: {
    backgroundColor: '#f9fafb',
    borderRadius: 12,
    padding: 16,
    borderWidth: 1,
    borderColor: '#e5e7eb',
  },
  siteItemSelected: {
    backgroundColor: '#eff6ff',
    borderColor: '#3b82f6',
  },
  siteItemContent: {
    gap: 8,
  },
  siteHeader: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    gap: 12,
  },
  siteNumber: {
    width: 24,
    height: 24,
    borderRadius: 12,
    backgroundColor: '#10b981',
    justifyContent: 'center',
    alignItems: 'center',
  },
  siteNumberText: {
    color: '#ffffff',
    fontSize: 12,
    fontWeight: 'bold',
  },
  siteInfo: {
    flex: 1,
    gap: 4,
  },
  siteName: {
    fontSize: 16,
    fontWeight: '600',
    color: '#1f2937',
  },
  categoryBadge: {
    alignSelf: 'flex-start',
    paddingHorizontal: 8,
    paddingVertical: 2,
    borderRadius: 8,
  },
  categoryText: {
    fontSize: 10,
    fontWeight: '600',
    color: '#ffffff',
  },
  walkingTime: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
  },
  walkingTimeText: {
    fontSize: 12,
    color: '#6b7280',
  },
  siteDescription: {
    fontSize: 14,
    color: '#6b7280',
    lineHeight: 20,
    marginLeft: 36, // Align with site name
  },
  siteStats: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 12,
    marginLeft: 36, // Align with site name
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
  scrollViewContent: {
    paddingBottom: 50, // Add extra padding at bottom for better scrolling
  },
  flatListContent: {
    paddingHorizontal: 16,
    paddingVertical: 4,
    paddingBottom: 50,
  },
});