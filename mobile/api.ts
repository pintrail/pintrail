export type Artifact = {
  id: string;
  name: string;
  desc: string;
  lat: number | null;
  lng: number | null;
  image_urls: string[];
  tags: string[];
  children?: Artifact[];
};

export const MOCK: Artifact[] = [
  {
    id: '1',
    name: 'Old Chapel',
    desc: 'Built in 1884, the Old Chapel is one of the oldest buildings on campus and a beloved landmark recognized for its distinctive Victorian Gothic architecture.',
    lat: 42.38814,
    lng: -72.52601,
    image_urls: [
      'https://picsum.photos/seed/chapel1/800/500',
      'https://picsum.photos/seed/chapel2/800/500',
    ],
    tags: ['Historic', 'Architecture'],
    children: [{ id: '2', name: 'Fine Arts Center', desc: '', lat: 42.38970, lng: -72.52340, image_urls: [], tags: [] }],
  },
  {
    id: '2',
    name: 'Fine Arts Center',
    desc: 'Designed by Kevin Roche, the Fine Arts Center is a celebrated brutalist complex housing galleries, theaters, and studios for the arts.',
    lat: 42.38970,
    lng: -72.52340,
    image_urls: [
      'https://picsum.photos/seed/fac1/800/500',
      'https://picsum.photos/seed/fac2/800/500',
      'https://picsum.photos/seed/fac3/800/500',
    ],
    tags: ['Arts', 'Architecture'],
    children: [],
  },
  {
    id: '3',
    name: 'Campus Pond',
    desc: 'A tranquil natural pond at the heart of campus, surrounded by walking paths and a popular gathering spot for students and wildlife alike.',
    lat: 42.39030,
    lng: -72.52700,
    image_urls: [
      'https://picsum.photos/seed/pond1/800/500',
      'https://picsum.photos/seed/pond2/800/500',
    ],
    tags: ['Nature', 'Outdoors'],
    children: [],
  },
  {
    id: '4',
    name: 'W.E.B. Du Bois Library',
    desc: 'Standing 28 stories tall, the Du Bois Library is the tallest library in the United States and the centerpiece of the UMass Amherst campus.',
    lat: 42.39170,
    lng: -72.52710,
    image_urls: [
      'https://picsum.photos/seed/lib1/800/500',
      'https://picsum.photos/seed/lib2/800/500',
    ],
    tags: ['Academic', 'Historic'],
    children: [
      { id: '3', name: 'Campus Pond', desc: '', lat: 42.39030, lng: -72.52700, image_urls: [], tags: [] },
    ],
  },
  {
    id: '5',
    name: 'Mullins Center',
    desc: 'A multi-purpose arena hosting UMass athletics, concerts, and major campus events since its opening in 1993.',
    lat: 42.39470,
    lng: -72.52460,
    image_urls: [
      'https://picsum.photos/seed/mullins1/800/500',
    ],
    tags: ['Sports', 'Events'],
    children: [],
  },
];

export function listArtifacts(): Artifact[] {
  return MOCK;
}

export function getArtifact(id: string): Artifact | undefined {
  return MOCK.find(a => a.id === id);
}
