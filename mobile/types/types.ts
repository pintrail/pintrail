export interface Site {
    id: string
    name: string
    category: string
    location: {
      lat: number
      lng: number
    }
    address: string
    description: string
    walkingTime?: number // minutes from user location
    artifacts: Artifact[]
    connectedSites: string[] // site IDs
    qrCode?: string
  }
  
  export interface Artifact {
    id: string
    type: 'image' | 'audio' | 'video' | 'document' | 'text'
    title: string
    content: string // URL for media, text content for text type
    description?: string
    thumbnail?: string
  }
  
  export interface Tour {
    id: string
    name: string
    description: string
    siteIds: string[]
    estimatedDuration: number // minutes
    totalDistance: number // meters
    difficulty: 'Easy' | 'Moderate' | 'Challenging'
    isCustom?: boolean
  }
  
  export interface UserLocation {
    lat: number
    lng: number
  }
  
  export type ViewType = 'map' | 'tours' | 'search' | 'qr'