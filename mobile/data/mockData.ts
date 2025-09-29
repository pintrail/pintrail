import type { Site, Tour } from '../types/types'

export const mockSites: Site[] = [
  {
    id: '1',
    name: 'Morrill Science Center',
    category: 'Academic Building',
    location: { lat: 42.3601, lng: -71.0589 },
    address: '3 Campus Center Way',
    description: 'A historic science building that has been the center of scientific research and education for over a century. The building features beautiful neo-Gothic architecture and houses several important research laboratories.',
    walkingTime: 0,
    qrCode: 'MSC001',
    artifacts: [
      {
        id: '1a',
        type: 'image',
        title: 'Exterior View',
        content: 'https://images.unsplash.com/photo-1624295415119-4811300f901f?crop=entropy&cs=tinysrgb&fit=max&fm=jpg&ixid=M3w3Nzg4Nzd8MHwxfHNlYXJjaHwxfHxoaXN0b3JpY2FsJTIwdW5pdmVyc2l0eSUyMGNhbXB1cyUyMGJ1aWxkaW5nfGVufDF8fHx8MTc1ODkxNTExNHww&ixlib=rb-4.1.0&q=80&w=1080',
        description: 'The iconic facade of Morrill Science Center'
      },
      {
        id: '1b',
        type: 'text',
        title: 'Building History',
        content: 'Built in 1922, the Morrill Science Center was named after Justin Smith Morrill, the sponsor of the Morrill Land-Grant Acts. The building has been home to groundbreaking research in physics, chemistry, and biology.',
        description: 'Historical background of the building'
      },
      {
        id: '1c',
        type: 'audio',
        title: 'Guided Tour Audio',
        content: '/audio/morrill-tour.mp3',
        description: 'Listen to a narrated tour of the building'
      }
    ],
    connectedSites: ['2', '3']
  },
  {
    id: '2',
    name: 'Integrative Learning Center',
    category: 'Learning Center',
    location: { lat: 42.3595, lng: -71.0580 },
    address: '650 North Pleasant Street',
    description: 'A state-of-the-art learning facility designed to promote collaborative and innovative education. The building features flexible learning spaces and advanced technology integration.',
    walkingTime: 4,
    qrCode: 'ILC002',
    artifacts: [
      {
        id: '2a',
        type: 'image',
        title: 'Modern Architecture',
        content: 'https://images.unsplash.com/photo-1710266211516-b300b6287ded?crop=entropy&cs=tinysrgb&fit=max&fm=jpg&ixid=M3w3Nzg4Nzd8MHwxfHNlYXJjaHwxfHxsZWFybmluZyUyMGNlbnRlciUyMG1vZGVybiUyMGJ1aWxkaW5nfGVufDF8fHx8MTc1ODkxNTEyMHww&ixlib=rb-4.1.0&q=80&w=1080',
        description: 'The contemporary design of the Integrative Learning Center'
      },
      {
        id: '2b',
        type: 'video',
        title: 'Virtual Tour',
        content: '/video/ilc-virtual-tour.mp4',
        description: 'Take a virtual tour of the learning spaces'
      }
    ],
    connectedSites: ['1', '4']
  },
  {
    id: '3',
    name: 'Puffers Pond',
    category: 'Natural Site',
    location: { lat: 42.3590, lng: -71.0595 },
    address: 'North Amherst',
    description: 'A beautiful natural pond surrounded by walking trails. Popular spot for students and faculty to relax and enjoy nature between classes.',
    walkingTime: 9,
    qrCode: 'PP003',
    artifacts: [
      {
        id: '3a',
        type: 'image',
        title: 'Pond View',
        content: 'https://images.unsplash.com/photo-1740596400203-1a92a9c786ff?crop=entropy&cs=tinysrgb&fit=max&fm=jpg&ixid=M3w3Nzg4Nzd8MHwxfHNlYXJjaHwxfHxzY2llbmNlJTIwbXVzZXVtJTIwYnVpbGRpbmclMjBleHRlcmlvcnxlbnwxfHx8fDE3NTg5MTUxMTd8MA&ixlib=rb-4.1.0&q=80&w=1080',
        description: 'Peaceful waters of Puffers Pond'
      },
      {
        id: '3b',
        type: 'text',
        title: 'Ecosystem Information',
        content: 'Puffers Pond is home to various species of waterfowl and native plants. The area serves as an outdoor classroom for environmental science students.',
        description: 'Learn about the local ecosystem'
      }
    ],
    connectedSites: ['1']
  },
  {
    id: '4',
    name: 'Old Belchertown Road',
    category: 'Historic Route',
    location: { lat: 42.3585, lng: -71.0575 },
    address: 'Old Belchertown Road',
    description: 'A historic road that once connected the campus to the town of Belchertown. The route has significant historical importance in the development of the area.',
    walkingTime: 0,
    qrCode: 'OBR004',
    artifacts: [
      {
        id: '4a',
        type: 'text',
        title: 'Road History',
        content: 'This historic route dates back to the 1800s and was a main thoroughfare for early settlers moving between communities.',
        description: 'Historical significance of the old road'
      },
      {
        id: '4b',
        type: 'document',
        title: 'Historical Map',
        content: '/documents/historical-map-1850.pdf',
        description: 'View the 1850 map showing the original road layout'
      }
    ],
    connectedSites: ['2', '5']
  },
  {
    id: '5',
    name: 'Courthouse via Belchertown Center',
    category: 'Historic Building',
    location: { lat: 42.3580, lng: -71.0570 },
    address: 'Belchertown Center',
    description: 'Historic courthouse building that served the local community for over 150 years. Features beautiful colonial architecture and houses important historical documents.',
    walkingTime: 18,
    qrCode: 'CH005',
    artifacts: [
      {
        id: '5a',
        type: 'image',
        title: 'Colonial Architecture',
        content: 'https://images.unsplash.com/photo-1624295415119-4811300f901f?crop=entropy&cs=tinysrgb&fit=max&fm=jpg&ixid=M3w3Nzg4Nzd8MHwxfHNlYXJjaHwxfHxoaXN0b3JpY2FsJTIwdW5pdmVyc2l0eSUyMGNhbXB1cyUyMGJ1aWxkaW5nfGVufDF8fHx8MTc1ODkxNTExNHww&ixlib=rb-4.1.0&q=80&w=1080',
        description: 'The historic courthouse building'
      },
      {
        id: '5b',
        type: 'text',
        title: 'Legal History',
        content: 'This courthouse has presided over significant legal cases in the region and houses archives dating back to 1800.',
        description: 'The building\'s role in local legal history'
      }
    ],
    connectedSites: ['4']
  }
]

export const mockTours: Tour[] = [
  {
    id: 'tour1',
    name: 'Academic Heritage Tour',
    description: 'Explore the historic academic buildings and learn about the university\'s educational legacy.',
    siteIds: ['1', '2'],
    estimatedDuration: 45,
    totalDistance: 800,
    difficulty: 'Easy'
  },
  {
    id: 'tour2',
    name: 'Nature & History Walk',
    description: 'A scenic route combining natural beauty with historical significance.',
    siteIds: ['3', '4', '5'],
    estimatedDuration: 90,
    totalDistance: 2400,
    difficulty: 'Moderate'
  },
  {
    id: 'tour3',
    name: 'Complete Campus Experience',
    description: 'A comprehensive tour covering all major sites on campus.',
    siteIds: ['1', '2', '3', '4', '5'],
    estimatedDuration: 120,
    totalDistance: 3200,
    difficulty: 'Challenging'
  }
]
