import React, { useState, useEffect } from 'react';
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  TextInput,
  ScrollView,
  ActivityIndicator,
  Animated,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import { mockSites } from '../data/mockData';
import type { Site } from '../types/types';

interface QRScannerProps {
  onSiteFound?: (site: Site) => void;
}

export default function QRScanner({ onSiteFound }: QRScannerProps) {
  const [isScanning, setIsScanning] = useState(false);
  const [manualCode, setManualCode] = useState('');
  const [scanResult, setScanResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [flashEnabled, setFlashEnabled] = useState(false);

  const handleStartScan = () => {
    setIsScanning(true);
    setError(null);
    setScanResult(null);
    
    // Simulate QR code scanning
    setTimeout(() => {
      // Simulate successful scan of first site's QR code
      const mockQRCode = mockSites[0].qrCode;
      if (mockQRCode) {
        setScanResult(mockQRCode);
        findSiteByQRCode(mockQRCode);
      }
      setIsScanning(false);
    }, 3000);
  };

  const handleManualEntry = () => {
    if (manualCode.trim()) {
      findSiteByQRCode(manualCode.trim());
    }
  };

  const findSiteByQRCode = (qrCode: string) => {
    const site = mockSites.find(s => s.qrCode === qrCode);
    if (site) {
      onSiteFound?.(site);
      setScanResult(qrCode);
      setError(null);
    } else {
      setError(`No site found with QR code: ${qrCode}`);
      setScanResult(null);
    }
  };

  const stopScanning = () => {
    setIsScanning(false);
  };

  const toggleFlash = () => {
    setFlashEnabled(!flashEnabled);
  };

  const ScanningOverlay = () => {
    const animatedValue = new Animated.Value(0);
    
    useEffect(() => {
      if (isScanning) {
        Animated.loop(
          Animated.sequence([
            Animated.timing(animatedValue, {
              toValue: 1,
              duration: 2000,
              useNativeDriver: true,
            }),
            Animated.timing(animatedValue, {
              toValue: 0,
              duration: 0,
              useNativeDriver: true,
            }),
          ])
        ).start();
      }
    }, [isScanning]);

    const translateY = animatedValue.interpolate({
      inputRange: [0, 1],
      outputRange: [-100, 100],
    });

    return (
      <View style={styles.scannerViewport}>
        {isScanning ? (
          <View style={styles.scanningContainer}>
            <View style={styles.scanningFrame}>
              {/* Corner brackets */}
              <View style={[styles.corner, styles.topLeft]} />
              <View style={[styles.corner, styles.topRight]} />
              <View style={[styles.corner, styles.bottomLeft]} />
              <View style={[styles.corner, styles.bottomRight]} />
              
              {/* Scanning line */}
              <Animated.View 
                style={[
                  styles.scanningLine,
                  { transform: [{ translateY }] }
                ]} 
              />
            </View>
            
            <View style={styles.scanningTextContainer}>
              <Text style={styles.scanningText}>Scanning for QR codes...</Text>
              <ActivityIndicator 
                size="small" 
                color="#3b82f6" 
                style={styles.scanningSpinner}
              />
            </View>
          </View>
        ) : (
          <View style={styles.idleContainer}>
            <Ionicons name="qr-code-outline" size={64} color="#4b5563" />
            <Text style={styles.idleText}>Point camera at QR code</Text>
          </View>
        )}
      </View>
    );
  };

  return (
    <SafeAreaView style={styles.container}>
      {/* Header */}
      <View style={styles.header}>
        <Text style={styles.title}>QR Code Scanner</Text>
        <Text style={styles.subtitle}>
          Scan a QR code at any site to view its details
        </Text>
      </View>

      {/* Scanner Content */}
      <ScrollView style={styles.content} showsVerticalScrollIndicator={false}>
        {/* Camera Scanner */}
        <View style={styles.card}>
          <View style={styles.cardHeader}>
            <View style={styles.cardTitleContainer}>
              <Ionicons name="camera-outline" size={20} color="#ffffff" />
              <Text style={styles.cardTitle}>Camera Scanner</Text>
            </View>
          </View>
          
          <View style={styles.cardContent}>
            <ScanningOverlay />
            
            {/* Scanner Controls */}
            <View style={styles.controlsContainer}>
              {!isScanning ? (
                <TouchableOpacity 
                  style={styles.primaryButton}
                  onPress={handleStartScan}
                  activeOpacity={0.8}
                >
                  <Ionicons name="camera-outline" size={16} color="#ffffff" />
                  <Text style={styles.primaryButtonText}>Start Scanning</Text>
                </TouchableOpacity>
              ) : (
                <TouchableOpacity 
                  style={styles.secondaryButton}
                  onPress={stopScanning}
                  activeOpacity={0.8}
                >
                  <Ionicons name="close-outline" size={16} color="#ffffff" />
                  <Text style={styles.secondaryButtonText}>Stop Scanning</Text>
                </TouchableOpacity>
              )}
              
              <TouchableOpacity 
                style={[styles.iconButton, flashEnabled && styles.iconButtonActive]}
                onPress={toggleFlash}
                activeOpacity={0.8}
              >
                <Ionicons 
                  name={flashEnabled ? "flash" : "flash-outline"} 
                  size={16} 
                  color={flashEnabled ? "#3b82f6" : "#ffffff"} 
                />
              </TouchableOpacity>
            </View>
          </View>
        </View>

        {/* Manual Entry */}
        <View style={styles.card}>
          <View style={styles.cardHeader}>
            <View style={styles.cardTitleContainer}>
              <Ionicons name="qr-code-outline" size={20} color="#ffffff" />
              <Text style={styles.cardTitle}>Manual Entry</Text>
            </View>
          </View>
          
          <View style={styles.cardContent}>
            <Text style={styles.cardSubtitle}>
              Enter a QR code manually if scanning isn't working
            </Text>
            
            <View style={styles.inputContainer}>
              <TextInput
                value={manualCode}
                onChangeText={setManualCode}
                placeholder="Enter QR code (e.g. MSC001)"
                placeholderTextColor="#9ca3af"
                style={styles.textInput}
              />
              <TouchableOpacity 
                style={[
                  styles.findButton,
                  !manualCode.trim() && styles.findButtonDisabled
                ]}
                onPress={handleManualEntry}
                disabled={!manualCode.trim()}
                activeOpacity={0.8}
              >
                <Text style={styles.findButtonText}>Find Site</Text>
              </TouchableOpacity>
            </View>
          </View>
        </View>

        {/* Results/Status */}
        {scanResult && (
          <View style={styles.successCard}>
            <View style={styles.successContent}>
              <Ionicons name="qr-code" size={20} color="#10b981" />
              <Text style={styles.successText}>QR Code Found: {scanResult}</Text>
            </View>
          </View>
        )}

        {error && (
          <View style={styles.errorCard}>
            <Text style={styles.errorText}>{error}</Text>
          </View>
        )}

        {/* Available QR Codes for Testing */}
        <View style={styles.card}>
          <View style={styles.cardHeader}>
            <Text style={styles.cardTitle}>Test QR Codes</Text>
          </View>
          
          <View style={styles.cardContent}>
            <Text style={styles.cardSubtitle}>
              For testing, try these QR codes:
            </Text>
            
            <View style={styles.testCodesContainer}>
              {mockSites.map(site => site.qrCode && (
                <TouchableOpacity
                  key={site.id}
                  style={styles.testCodeButton}
                  onPress={() => setManualCode(site.qrCode!)}
                  activeOpacity={0.7}
                >
                  <Text style={styles.testCodeQR}>{site.qrCode}</Text>
                  <Text style={styles.testCodeName}>{site.name}</Text>
                </TouchableOpacity>
              ))}
            </View>
          </View>
        </View>
      </ScrollView>
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
  },
  content: {
    flex: 1,
    padding: 16,
  },
  card: {
    backgroundColor: '#1e293b', // slate-800
    borderWidth: 1,
    borderColor: '#374151', // gray-700
    borderRadius: 12,
    marginBottom: 24,
    overflow: 'hidden',
  },
  cardHeader: {
    padding: 16,
    borderBottomWidth: 1,
    borderBottomColor: '#374151',
  },
  cardTitleContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  cardTitle: {
    fontSize: 16,
    fontWeight: '600',
    color: '#ffffff',
  },
  cardContent: {
    padding: 16,
  },
  cardSubtitle: {
    fontSize: 14,
    color: '#9ca3af',
    marginBottom: 16,
  },
  scannerViewport: {
    aspectRatio: 1,
    backgroundColor: '#000000',
    borderRadius: 8,
    marginBottom: 16,
    overflow: 'hidden',
  },
  scanningContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  scanningFrame: {
    width: 192,
    height: 192,
    borderWidth: 2,
    borderColor: 'rgba(255, 255, 255, 0.2)',
    borderRadius: 8,
    position: 'relative',
  },
  corner: {
    position: 'absolute',
    width: 24,
    height: 24,
    borderColor: '#3b82f6',
  },
  topLeft: {
    top: -2,
    left: -2,
    borderTopWidth: 2,
    borderLeftWidth: 2,
  },
  topRight: {
    top: -2,
    right: -2,
    borderTopWidth: 2,
    borderRightWidth: 2,
  },
  bottomLeft: {
    bottom: -2,
    left: -2,
    borderBottomWidth: 2,
    borderLeftWidth: 2,
  },
  bottomRight: {
    bottom: -2,
    right: -2,
    borderBottomWidth: 2,
    borderRightWidth: 2,
  },
  scanningLine: {
    position: 'absolute',
    left: 0,
    right: 0,
    height: 2,
    backgroundColor: '#3b82f6',
    top: '50%',
  },
  scanningTextContainer: {
    alignItems: 'center',
    marginTop: 16,
  },
  scanningText: {
    color: '#ffffff',
    fontSize: 14,
  },
  scanningSpinner: {
    marginTop: 8,
  },
  idleContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  idleText: {
    color: '#9ca3af',
    fontSize: 14,
    marginTop: 16,
  },
  controlsContainer: {
    flexDirection: 'row',
    gap: 8,
  },
  primaryButton: {
    flex: 1,
    backgroundColor: '#3b82f6',
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 12,
    paddingHorizontal: 16,
    borderRadius: 8,
    gap: 8,
  },
  primaryButtonText: {
    color: '#ffffff',
    fontSize: 14,
    fontWeight: '600',
  },
  secondaryButton: {
    flex: 1,
    backgroundColor: 'transparent',
    borderWidth: 1,
    borderColor: '#4b5563',
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 12,
    paddingHorizontal: 16,
    borderRadius: 8,
    gap: 8,
  },
  secondaryButtonText: {
    color: '#ffffff',
    fontSize: 14,
    fontWeight: '600',
  },
  iconButton: {
    backgroundColor: 'transparent',
    borderWidth: 1,
    borderColor: '#4b5563',
    width: 48,
    height: 48,
    justifyContent: 'center',
    alignItems: 'center',
    borderRadius: 8,
  },
  iconButtonActive: {
    borderColor: '#3b82f6',
    backgroundColor: 'rgba(59, 130, 246, 0.1)',
  },
  inputContainer: {
    flexDirection: 'row',
    gap: 8,
  },
  textInput: {
    flex: 1,
    backgroundColor: '#374151', // slate-700
    borderWidth: 1,
    borderColor: '#4b5563', // gray-600
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 12,
    color: '#ffffff',
    fontSize: 14,
  },
  findButton: {
    backgroundColor: '#10b981', // green-600
    paddingHorizontal: 16,
    paddingVertical: 12,
    borderRadius: 8,
    justifyContent: 'center',
  },
  findButtonDisabled: {
    backgroundColor: '#374151',
    opacity: 0.5,
  },
  findButtonText: {
    color: '#ffffff',
    fontSize: 14,
    fontWeight: '600',
  },
  successCard: {
    backgroundColor: 'rgba(16, 185, 129, 0.1)', // green-900/50
    borderWidth: 1,
    borderColor: '#059669', // green-700
    borderRadius: 12,
    marginBottom: 24,
  },
  successContent: {
    padding: 16,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  successText: {
    color: '#10b981', // green-400
    fontSize: 14,
  },
  errorCard: {
    backgroundColor: 'rgba(220, 38, 38, 0.1)', // red-900/50
    borderWidth: 1,
    borderColor: '#b91c1c', // red-700
    borderRadius: 12,
    padding: 16,
    marginBottom: 24,
  },
  errorText: {
    color: '#f87171', // red-400
    fontSize: 14,
  },
  testCodesContainer: {
    gap: 8,
  },
  testCodeButton: {
    backgroundColor: '#374151', // slate-700
    padding: 12,
    borderRadius: 8,
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  testCodeQR: {
    color: '#ffffff',
    fontFamily: 'monospace',
    fontSize: 14,
    fontWeight: '600',
  },
  testCodeName: {
    color: '#9ca3af',
    fontSize: 12,
  },
});
