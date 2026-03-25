import { InMemoryArtifactRepository } from '#root/src/artifacts/repository/in-memory-repository.js'

const repo = InMemoryArtifactRepository()

const art = {
  name: 'Site Survey Photo',
  desc: 'Aerial photograph taken during the Q1 site survey',
  loc: {
    lat: 42.3601,
    lng: -71.0589,
  },
  createdAt: new Date('2026-01-15T09:30:00Z'),
  updatedAt: new Date('2026-03-10T14:22:00Z'),
}

const no_art = {
  name: 'Site Survey Photo',
  loc: {
    lat: 42.3601,
    lng: -71.0589,
  },
  createdAt: new Date('2026-01-15T09:30:00Z'),
  updatedAt: new Date('2026-03-10T14:22:00Z'),
}

const res = repo.add(art)
console.log(res)

const no_res = repo.add(no_art)
console.log(no_res)

const x = repo.get(res.v.id)
console.log(x.getOrElse(`Could not find artifact with id ${res.v.id}`))
