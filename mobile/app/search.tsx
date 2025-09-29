import React, { useState, useMemo } from 'react';
import {
  View,
  Text,
  StyleSheet,
  TextInput,
  ScrollView,
  TouchableOpacity,
  Image,
  FlatList,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import { mockSites } from '../data/mockData';
import type { Site } from '../types/types';

interface SearchViewProps {
  sites?: Site[];
  onSiteSelect?: (site: Site) => void;
}

export default function SearchView({ sites = mockSites, onSiteSelect }: SearchViewProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedCategory, setSelectedCategory] = useState<string>('all');
  const [sortBy, setSortBy] = useState<string>('name');
  const [showCategoryPicker, setShowCategoryPicker] = useState(false);
  const [showSortPicker, setShowSortPicker] = useState(false);

  const categories = useMemo(() => {
    const cats = Array.from(new Set(sites.map(site => site.category)));
    return ['all', ...cats];
  }, [sites]);

  const filteredAndSortedSites = useMemo(() => {
    let filtered = sites.filter(site => {
      const matchesSearch = searchQuery === '' ||
        site.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        site.description.toLowerCase().includes(searchQuery.toLowerCase()) ||
        site.category.toLowerCase().includes(searchQuery.toLowerCase()) ||
        site.address.toLowerCase().includes(searchQuery.toLowerCase());

      const matchesCategory = selectedCategory === 'all' || site.category === selectedCategory;

      return matchesSearch && matchesCategory;
    });

    // Sort the filtered results
    filtered.sort((a, b) => {
      switch (sortBy) {
        case 'name':
          return a.name.localeCompare(b.name);
        case 'category':
          return a.category.localeCompare(b.category);
        case 'walkingTime':
          return (a.walkingTime || 0) - (b.walkingTime || 0);
        default:
          return 0;
      }
    });

    return filtered;
  }, [sites, searchQuery, selectedCategory, sortBy]);

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

  const getFirstImage = (site: Site) => {
    const imageArtifact = site.artifacts.find(artifact => artifact.type === 'image');
    return imageArtifact?.content;
  };

  const clearFilters = () => {
    setSearchQuery('');
    setSelectedCategory('all');
  };

  const renderCategoryBadge = (category: string) => (
    <View style={[styles.badge, { backgroundColor: getCategoryColor(category) }]}>
      <Text style={styles.badgeText}>{category}</Text>
    </View>
  );

  const renderSiteCard = ({ item: site }: { item: Site }) => (
    <TouchableOpacity
      style={styles.siteCard}
      onPress={() => onSiteSelect?.(site)}
      activeOpacity={0.7}
    >
      <View style={styles.siteCardContent}>
        {/* Site Image */}
        <View style={styles.imageContainer}>
          {getFirstImage(site) ? (
            <Image
              source={{ uri: getFirstImage(site)! }}
              style={styles.siteImage}
              resizeMode="cover"
            />
          ) : (
            <View style={[styles.fallbackImage, { backgroundColor: getCategoryColor(site.category) }]}>
              <Ionicons name="location-outline" size={24} color="#ffffff" />
            </View>
          )}
        </View>

        {/* Site Info */}
        <View style={styles.siteInfo}>
          <View style={styles.siteHeader}>
            <Text style={styles.siteName} numberOfLines={1}>{site.name}</Text>
            {site.walkingTime !== undefined && site.walkingTime > 0 && (
              <View style={styles.walkingTimeContainer}>
                <Ionicons name="time-outline" size={12} color="#9ca3af" />
                <Text style={styles.walkingTime}>{site.walkingTime}min</Text>
              </View>
            )}
          </View>

          {renderCategoryBadge(site.category)}

          <Text style={styles.siteDescription} numberOfLines={2}>
            {site.description}
          </Text>

          <View style={styles.siteStats}>
            <Text style={styles.statText}>
              {site.artifacts.length} artifact{site.artifacts.length !== 1 ? 's' : ''}
            </Text>
            {site.connectedSites.length > 0 && (
              <Text style={styles.statText}>
                {site.connectedSites.length} connected site{site.connectedSites.length !== 1 ? 's' : ''}
              </Text>
            )}
          </View>
        </View>
      </View>
    </TouchableOpacity>
  );

  const renderEmptyState = () => (
    <View style={styles.emptyState}>
      <Ionicons name="search-outline" size={48} color="#4b5563" />
      <Text style={styles.emptyTitle}>No sites found</Text>
      <Text style={styles.emptySubtitle}>Try adjusting your search or filters</Text>
    </View>
  );

  return (
    <SafeAreaView style={styles.container}>
      {/* Header */}
      <View style={styles.header}>
        <Text style={styles.title}>Search Sites</Text>

        {/* Search Input */}
        <View style={styles.searchContainer}>
          <Ionicons name="search-outline" size={16} color="#9ca3af" style={styles.searchIcon} />
          <TextInput
            value={searchQuery}
            onChangeText={setSearchQuery}
            placeholder="Search sites, categories, or descriptions..."
            placeholderTextColor="#9ca3af"
            style={styles.searchInput}
          />
        </View>

        {/* Filters */}
        <View style={styles.filtersContainer}>
          <TouchableOpacity
            style={styles.filterButton}
            onPress={() => setShowCategoryPicker(!showCategoryPicker)}
          >
            <Text style={styles.filterButtonText}>
              {selectedCategory === 'all' ? 'All Categories' : selectedCategory}
            </Text>
            <Ionicons name="chevron-down-outline" size={16} color="#9ca3af" />
          </TouchableOpacity>

          <TouchableOpacity
            style={styles.filterButton}
            onPress={() => setShowSortPicker(!showSortPicker)}
          >
            <Text style={styles.filterButtonText}>
              Sort: {sortBy === 'name' ? 'Name' : sortBy === 'category' ? 'Category' : 'Distance'}
            </Text>
            <Ionicons name="chevron-down-outline" size={16} color="#9ca3af" />
          </TouchableOpacity>
        </View>

        {/* Category Picker */}
        {showCategoryPicker && (
          <View style={styles.picker}>
            <ScrollView style={styles.pickerScroll}>
              {categories.map(category => (
                <TouchableOpacity
                  key={category}
                  style={styles.pickerItem}
                  onPress={() => {
                    setSelectedCategory(category);
                    setShowCategoryPicker(false);
                  }}
                >
                  <Text style={styles.pickerItemText}>
                    {category === 'all' ? 'All Categories' : category}
                  </Text>
                </TouchableOpacity>
              ))}
            </ScrollView>
          </View>
        )}

        {/* Sort Picker */}
        {showSortPicker && (
          <View style={styles.picker}>
            {['name', 'category', 'walkingTime'].map(option => (
              <TouchableOpacity
                key={option}
                style={styles.pickerItem}
                onPress={() => {
                  setSortBy(option);
                  setShowSortPicker(false);
                }}
              >
                <Text style={styles.pickerItemText}>
                  {option === 'name' ? 'Name' : option === 'category' ? 'Category' : 'Distance'}
                </Text>
              </TouchableOpacity>
            ))}
          </View>
        )}

        {/* Active Filters */}
        {(searchQuery || selectedCategory !== 'all') && (
          <View style={styles.activeFilters}>
            <Text style={styles.activeFiltersLabel}>Active filters:</Text>
            {searchQuery && (
              <View style={styles.filterTag}>
                <Text style={styles.filterTagText}>"{searchQuery}"</Text>
              </View>
            )}
            {selectedCategory !== 'all' && (
              <View style={styles.filterTag}>
                <Text style={styles.filterTagText}>{selectedCategory}</Text>
              </View>
            )}
            <TouchableOpacity onPress={clearFilters}>
              <Text style={styles.clearFiltersText}>Clear all</Text>
            </TouchableOpacity>
          </View>
        )}
      </View>

      {/* Results */}
      <View style={styles.resultsContainer}>
        {filteredAndSortedSites.length === 0 ? (
          renderEmptyState()
        ) : (
          <>
            <View style={styles.resultsHeader}>
              <Text style={styles.resultsCount}>
                {filteredAndSortedSites.length} site{filteredAndSortedSites.length !== 1 ? 's' : ''} found
              </Text>
            </View>

            <FlatList
              data={filteredAndSortedSites}
              renderItem={renderSiteCard}
              keyExtractor={(item) => item.id}
              style={styles.sitesList}
              showsVerticalScrollIndicator={false}
              contentContainerStyle={styles.sitesListContent}
            />
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
    marginBottom: 16,
  },
  searchContainer: {
    position: 'relative',
    marginBottom: 16,
  },
  searchIcon: {
    position: 'absolute',
    left: 12,
    top: 12,
    zIndex: 1,
  },
  searchInput: {
    backgroundColor: '#1e293b', // slate-800
    borderWidth: 1,
    borderColor: '#4b5563', // gray-600
    borderRadius: 8,
    paddingLeft: 40,
    paddingRight: 16,
    paddingVertical: 12,
    color: '#ffffff',
    fontSize: 16,
  },
  filtersContainer: {
    flexDirection: 'row',
    gap: 8,
    marginBottom: 8,
  },
  filterButton: {
    flex: 1,
    backgroundColor: '#1e293b', // slate-800
    borderWidth: 1,
    borderColor: '#4b5563', // gray-600
    borderRadius: 8,
    padding: 12,
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  filterButtonText: {
    color: '#ffffff',
    fontSize: 14,
    flex: 1,
  },
  picker: {
    backgroundColor: '#1e293b', // slate-800
    borderWidth: 1,
    borderColor: '#4b5563', // gray-600
    borderRadius: 8,
    marginTop: 8,
    maxHeight: 200,
  },
  pickerScroll: {
    maxHeight: 200,
  },
  pickerItem: {
    padding: 12,
    borderBottomWidth: 1,
    borderBottomColor: '#374151',
  },
  pickerItemText: {
    color: '#ffffff',
    fontSize: 14,
  },
  activeFilters: {
    flexDirection: 'row',
    alignItems: 'center',
    flexWrap: 'wrap',
    gap: 8,
    marginTop: 8,
  },
  activeFiltersLabel: {
    color: '#9ca3af',
    fontSize: 12,
  },
  filterTag: {
    backgroundColor: '#374151', // slate-700
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 12,
  },
  filterTagText: {
    color: '#ffffff',
    fontSize: 12,
  },
  clearFiltersText: {
    color: '#9ca3af',
    fontSize: 12,
    textDecorationLine: 'underline',
  },
  resultsContainer: {
    flex: 1,
    padding: 16,
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
  },
  resultsHeader: {
    marginBottom: 16,
  },
  resultsCount: {
    color: '#9ca3af',
    fontSize: 14,
  },
  sitesList: {
    flex: 1,
  },
  sitesListContent: {
    gap: 16,
  },
  siteCard: {
    backgroundColor: '#1e293b', // slate-800
    borderWidth: 1,
    borderColor: '#374151', // gray-700
    borderRadius: 12,
    overflow: 'hidden',
  },
  siteCardContent: {
    padding: 16,
    flexDirection: 'row',
    gap: 16,
  },
  imageContainer: {
    width: 64,
    height: 64,
    borderRadius: 8,
    overflow: 'hidden',
  },
  siteImage: {
    width: '100%',
    height: '100%',
  },
  fallbackImage: {
    width: '100%',
    height: '100%',
    justifyContent: 'center',
    alignItems: 'center',
  },
  siteInfo: {
    flex: 1,
    gap: 8,
  },
  siteHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'flex-start',
  },
  siteName: {
    fontSize: 16,
    fontWeight: '600',
    color: '#ffffff',
    flex: 1,
    marginRight: 8,
  },
  walkingTimeContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
  },
  walkingTime: {
    color: '#9ca3af',
    fontSize: 12,
  },
  badge: {
    alignSelf: 'flex-start',
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 12,
  },
  badgeText: {
    color: '#ffffff',
    fontSize: 12,
    fontWeight: '500',
  },
  siteDescription: {
    color: '#9ca3af',
    fontSize: 14,
    lineHeight: 20,
  },
  siteStats: {
    flexDirection: 'row',
    gap: 16,
  },
  statText: {
    color: '#6b7280',
    fontSize: 12,
  },
});
