import type { Site, Tour } from '../types/types'

export const mockSites: Site[] = [
  {
    id: '1',
    name: 'Morrill Science Center',
    category: 'Academic Building',
    location: { lat: 42.390882, lng: -72.524793 },
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
    location: { lat: 42.390827, lng: -72.525872 },
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
    location: { lat: 42.389406, lng: -72.526891 },
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
    name: 'Fine Arts Center',
    category: 'Academic Building',
    location: { lat: 42.388196, lng: -72.525767 },
    address: '151 Presidents Drive',
    description: 'A vibrant hub for creative expression featuring galleries, performance spaces, and studios. Home to the university\'s art, music, and theater programs with rotating exhibitions and student showcases.',
    walkingTime: 6,
    qrCode: 'FAC004',
    artifacts: [
      {
        id: '4a',
        type: 'image',
        title: 'Gallery Exhibition',
        content: 'https://images.unsplash.com/photo-1578662996442-48f60103fc96?crop=entropy&cs=tinysrgb&fit=max&fm=jpg&ixid=M3w3Nzg4Nzd8MHwxfHNlYXJjaHwxfHxhcnQlMjBnYWxsZXJ5JTIwZXhoaWJpdGlvbnxlbnwxfHx8fDE3NTg5MTUxMjN8MA&ixlib=rb-4.1.0&q=80&w=1080',
        description: 'Current student art exhibition in the main gallery'
      },
      {
        id: '4b',
        type: 'video',
        title: 'Performance Highlights',
        content: '/video/fac-performances.mp4',
        description: 'Watch highlights from recent theater and music performances'
      },
      {
        id: '4c',
        type: 'audio',
        title: 'Artist Interview',
        content: '/audio/artist-spotlight.mp3',
        description: 'Listen to interviews with featured student artists'
      },
      {
        id: '4d',
        type: 'text',
        title: 'Building Features',
        content: 'The Fine Arts Center includes a 400-seat concert hall, multiple galleries, dance studios, ceramics workshop, printmaking lab, and digital media suites. The building hosts over 200 events annually.',
        description: 'Learn about the center\'s facilities and programs'
      }
    ],
    connectedSites: ['2']
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
    siteIds: ['3', '4'],
    estimatedDuration: 90,
    totalDistance: 2400,
    difficulty: 'Moderate'
  },
  {
    id: 'tour3',
    name: 'Complete Campus Experience',
    description: 'A comprehensive tour covering all major sites on campus.',
    siteIds: ['1', '2', '3', '4'],
    estimatedDuration: 120,
    totalDistance: 3200,
    difficulty: 'Challenging'
  },
  {
    id: 'tour4',
    name: 'Creative Arts Discovery',
    description: 'Immerse yourself in the artistic heart of campus. Explore galleries, performance spaces, and creative studios at the Fine Arts Center.',
    siteIds: ['4'],
    estimatedDuration: 30,
    totalDistance: 400,
    difficulty: 'Easy'
  }
]
