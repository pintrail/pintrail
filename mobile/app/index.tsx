import React, { useState, useRef, useMemo, useCallback } from 'react';
import MapView, { Region, Marker } from 'react-native-maps';
import {
  StyleSheet,
  View,
  Text,
  TouchableOpacity,
  Modal,
  ScrollView
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import BottomSheet, { BottomSheetView, BottomSheetFlatList } from '@gorhom/bottom-sheet';
import { GestureHandlerRootView, GestureDetector, Gesture } from 'react-native-gesture-handler';
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
  const [showSiteModal, setShowSiteModal] = useState(false);

  // Bottom sheet refs and snap points
  const bottomSheetRef = useRef<BottomSheet>(null);
  const snapPoints = useMemo(() => ['10%', '50%'], []);

  const onRegionChangeComplete = (region: Region) => {
    setRegion(region);
  };

  const handleMarkerPress = useCallback((site: Site) => {
    setSelectedSite(site);
    setShowSiteModal(true);
  }, []);

  const handleSheetChanges = useCallback((index: number) => {
    // Handle sheet changes if needed
  }, []);

  // Create native gesture for map
  const nativeGesture = Gesture.Native();


  const getCategoryColor = (category: string) => {
    switch (category) {
      case 'Academic Building': return 'red';
      case 'Learning Center': return 'blue';
      case 'Natural Site': return 'green';
      default: return 'red';
    }
  };

  const getCategoryBadgeColor = (category: string) => {
    switch (category) {
      case 'Academic Building': return '#dc2626';
      case 'Learning Center': return '#2563eb';
      case 'Natural Site': return '#059669';
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
        // Expand bottom sheet to show site details
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
            <View style={[styles.categoryBadge, { backgroundColor: getCategoryBadgeColor(site.category) }]}>
              <Text style={styles.categoryBadgeText}>{site.category}</Text>
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
      <GestureDetector gesture={nativeGesture}>
        <MapView
          style={styles.map}
          region={region}
          onRegionChangeComplete={onRegionChangeComplete}
          showsUserLocation={true}
          showsMyLocationButton={false}
        >
          {mockSites.map((site, index) => (
            <Marker
              key={site.id}
              coordinate={{
                latitude: site.location.lat,
                longitude: site.location.lng,
              }}
              title={site.name}
              description={site.category}
              pinColor={getCategoryColor(site.category)}
              onPress={() => handleMarkerPress(site)}
            />
          ))}
        </MapView>
      </GestureDetector>

      {/* Bottom Sheet for Site List */}
      <BottomSheet
        ref={bottomSheetRef}
        index={0}
        snapPoints={snapPoints}
        onChange={handleSheetChanges}
        enablePanDownToClose={false}
        handleIndicatorStyle={styles.sheetHandle}
        backgroundStyle={styles.bottomSheetBackground}
        enableContentPanningGesture={false}
        enableHandlePanningGesture={true}
        enableOverDrag={false}
        overDragResistanceFactor={0}
      >
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
      </BottomSheet>

      {/* Site Details Modal */}
      <Modal
        visible={showSiteModal}
        animationType="slide"
        presentationStyle="pageSheet"
        onRequestClose={() => setShowSiteModal(false)}
      >
        <SafeAreaView style={styles.modalContainer}>
          <View style={styles.modalHeader}>
            <Text style={styles.modalTitle}>
              {selectedSite?.name || 'Site Details'}
            </Text>
            <TouchableOpacity
              onPress={() => setShowSiteModal(false)}
              style={styles.closeButton}
            >
              <Ionicons name="close" size={24} color="#374151" />
            </TouchableOpacity>
          </View>

          {selectedSite && (
            <ScrollView style={styles.modalContent}>
              <Text style={styles.categoryText}>
                {selectedSite.category}
              </Text>

              <Text style={styles.descriptionText}>
                {selectedSite.description}
              </Text>

              <View style={styles.detailsSection}>
                <View style={styles.detailRow}>
                  <Ionicons name="location-outline" size={20} color="#6b7280" />
                  <Text style={styles.detailText}>{selectedSite.address}</Text>
                </View>

                {selectedSite.walkingTime !== undefined && selectedSite.walkingTime > 0 && (
                  <View style={styles.detailRow}>
                    <Ionicons name="time-outline" size={20} color="#6b7280" />
                    <Text style={styles.detailText}>{selectedSite.walkingTime} minutes walk</Text>
                  </View>
                )}

                <View style={styles.detailRow}>
                  <Ionicons name="albums-outline" size={20} color="#6b7280" />
                  <Text style={styles.detailText}>
                    {selectedSite.artifacts.length} artifact{selectedSite.artifacts.length !== 1 ? 's' : ''} available
                  </Text>
                </View>

                {selectedSite.qrCode && (
                  <View style={styles.detailRow}>
                    <Ionicons name="qr-code-outline" size={20} color="#6b7280" />
                    <Text style={styles.detailText}>QR Code: {selectedSite.qrCode}</Text>
                  </View>
                )}
              </View>
            </ScrollView>
          )}
        </SafeAreaView>
      </Modal>
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
    backgroundColor: '#9ca3af',
    width: 50,
    height: 5,
    borderRadius: 3,
  },
  sheetHeader: {
    paddingHorizontal: 16,
    paddingTop: 0,
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
  flatListContent: {
    paddingHorizontal: 16,
    paddingVertical: 4,
    paddingBottom: 50,
  },
  modalContainer: {
    flex: 1,
    backgroundColor: '#ffffff',
  },
  modalHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: 16,
    borderBottomWidth: 1,
    borderBottomColor: '#e5e7eb',
  },
  modalTitle: {
    fontSize: 20,
    fontWeight: 'bold',
    color: '#111827',
    flex: 1,
  },
  closeButton: {
    padding: 8,
  },
  modalContent: {
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
    gap: 16,
  },
  detailRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  detailText: {
    fontSize: 16,
    color: '#374151',
    flex: 1,
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
    paddingHorizontal: 10,
    paddingVertical: 4,
    borderRadius: 12,
  },
  categoryBadgeText: {
    fontSize: 11,
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
});